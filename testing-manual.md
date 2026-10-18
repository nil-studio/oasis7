# Workflow Reference (Single Source)
- 本手册仅定义测试分层、套件入口和操作细节。
- 流程阶段、责任边界、必需/可选 gate、失败回退路径统一以 `doc/engineering/workflow/source-of-truth.md` 为准。
- 如测试流程规则与 source-of-truth 冲突，以 source-of-truth 为准，并先更新 source-of-truth。

# oasis7: 系统性应用测试手册（Human/AI 通用）

## 目标
- 基于仓库当前实现，提供一套可直接执行的分层测试手册，让人类开发者与 AI Agent 都能对“整应用”做足够充分的测试。
- 解决“只跑一条命令看总绿灯”但无法定位风险层的问题，把测试明确拆成基础门禁、核心逻辑、协议集成、分布式子系统、UI 闭环、压力回归。
- 把 `test_tier_required` 与 `test_tier_full` 放回整体测试体系中：它们是核心层基线，不等于“整应用全覆盖”。
- 统一证据标准（命令、日志、截图、结论），保证测试可复盘、可审计。

## 对标入口
- 本手册负责 oasis7 自己的执行分层与命令入口；当前 `public_testnet` readiness 以本手册的 Network Tiers 入口和 formal `public_testnet` runbook 为准。
- 主流公链成熟度 benchmark 与 legacy network-rehearsal / release-train 专题只作对标和历史 evidence 追溯，不作为当前 `public_testnet`、`mainnet` 或统一持久大世界上线结论。

## 范围

### In Scope
- 结合当前仓库真实实现给出分层模型与命令清单。
- 给出“改动路径 -> 应跑测试层级”的触发矩阵。
- 给出 Human/AI 共用执行剧本、通过标准、失败分诊与证据规范。
- 明确现有 CI 覆盖能力与手册补充覆盖能力的边界。

### Out of Scope
- 不在本任务修改 CI workflow 或测试脚本行为。
- 不引入新的测试框架或新的业务代码。
- 不做覆盖率百分比硬门槛治理（如行覆盖率 >= N%）。

## TCP conformance 完成条件

Hosted fresh admission、metadata release 和 ordinary cadence 的功能验证等待原 canonical receipt、settled lease，以及有 memory intent 的双 turn 对应 memory，不把原来的三、四、八秒观察窗口当作产品 SLA。这三个用例的父进程使用 `Instant` 执行最多 60 秒的本地单调硬截止；跨进程通过 `PRE2_CONFORMANCE_COMPLETION_DEADLINE_MS` 传递 wall-clock epoch 截止，child 将剩余预算转为本地 `Instant`，nested launcher 另预留 10 秒清理。因此不能将跨进程预算描述为共享同一个纯单调时钟。父进程以 `create_new` 建立 regular log 文件，stdout 和 stderr 各保留最多 2 MiB tail，避免继承 pipe 阻塞；`PRE2_CONFORMANCE_EVIDENCE_DIR` 可保留完整日志，超时只终止自有 child。真实 pump、memory 或 terminal rejection 提前失败；metadata-held 的零提交断言、协议顺序和双 turn identity/nonce 验证仍保留。

日志仍必须满足私有性要求：测试根目录和归档目录应仅允许当前用户访问，日志应仅允许当前用户读写，并在运行前后核验权限（Unix 目录 `0700` / 文件 `0600`，其他平台须有等效 ACL）。当前这三个入口的普通 `create_dir_all` 与 log `create_new` 没有显式收窄权限，不能单凭这些 API 保证私有；是否可被其他用户读取取决于 umask、父目录与 ACL。未补齐并验证私有创建或等效环境前置时，私有性验收保持未满足，不得将功能测试通过当作权限通过；完整日志归档也须保留同等保护。本说明不代表已观察到实际泄露。

因果延迟探针只在已验证成功的 View 响应上等待，响应字节保持原样。串行运行目标用例时，可设置 `PRE2_CONFORMANCE_VIEW_DELAY_MS=600`（低于 transport timeout），分别过滤 `real_tcp_hosted_fresh_admission_observes_reserves_and_invokes_provider`、`real_tcp_fresh_admission_waits_for_successful_metadata` 和 `real_tcp_hosted_ordinary_cadence_commits_two_distinct_turns`。日志记录原请求和阶段推进；通过仍须获得同一原请求的 canonical result、唯一结算；双 turn 还须保留各自 memory。该探针验证多阶段累计延迟，不是吞吐或性能验收。

## 开发态缓存约定
- 若当前是在同一 repo family 的多个 git worktree 之间做本地迭代，开发态 `cargo check/test/run/build` 默认优先使用 `./scripts/cargo-dev.sh <cargo-args...>`。每个 worktree 按自身 source identity 使用稳定、隔离的 target namespace；同一 worktree 内可复用缓存，divergent worktree 不得互选 artifacts。通过 `./scripts/new-task-worktree.sh` 创建的新 task worktree 会把 git-ignored `target` 链接到该 worktree 的 namespace，使直接 cargo 与 wrapper 在同一 worktree 内复用同一开发态缓存。
- 本地 smoke / playtest / prewarm / regression / drill / longrun 脚本若只是为了开发反馈，应优先 source `scripts/cargo-dev-lib.sh` 并调用 `oasis7_cargo_dev ...` / `oasis7_cargo_dev_debug_bin_dir`，从而与手工 `cargo-dev.sh` 使用同一个当前-worktree target；`CI=1`、`OASIS7_CARGO_DEV_SHARED=0` 或 `OASIS7_FORCE_RAW_CARGO=1` 会回退到原始 cargo target 解析。
- 该入口只服务开发态缓存复用，不替代本手册中的正式验收命令；手册里的 canonical 验收命令仍显式写原始 `env -u RUSTC_WRAPPER cargo ...`。
- 跨 worktree 的 registry 依赖编译结果复用由 sccache 提供，不共用 target artifacts。安装 `sccache`（macOS: `brew install sccache`）后，`cargo-dev.sh build/test/run` 自动启用它、通过参数传递 target（避免 `CARGO_TARGET_DIR` 污染缓存键）并关闭该调用的 Rust incremental；所有本地开发调用（含 `check/clippy`，以及没有 sccache 时）统一设置 dev/test debug=0、incremental=0。`OASIS7_CARGO_DEV_MODE=debug` 显式启用 debug=2、incremental=1 并关闭 sccache；不要在同一轮日常迭代反复切换模式，以免重编译。CI 保留原有配置。`OASIS7_CARGO_SCCACHE=0` 或 CI 可禁用，未安装时沿用 Cargo。用 `sccache --show-stats` 查看命中；首次编译填充缓存，workspace/path crates 的源码路径仍进入 Rust cache key，最终链接、proc-macro 和 check 不保证命中。wrapper 首次本地调用会为缺失的 `target` 建立当前 namespace 链接；已有工作区可执行 `./scripts/cargo-dev.sh --prepare-target`，之后直接 Cargo 也复用该目录。已有普通目录须确认未使用并清理后再准备，脚本不会自动删除它。Windows 缺少符号链接权限时，普通 wrapper 调用仍通过参数使用共享缓存；`--prepare-target` 则报告失败，直接 Cargo 的链接复用需先启用符号链接权限。
- 磁盘有限时保留少量固定路径开发 worktree，串行任务在确认干净、独占且已保留未推送工作的工作区用 `git switch -c codex/<task> <base>` 切换独立分支；仅并行任务新建 worktree。不要编辑 main 或主工作区。`new-task-worktree.sh --with-harness` 是完整预热的显式选项，普通后端或文档任务不需要它。
- 工作区选择先检查 `git worktree list --porcelain`，不能仅看当前聊天附件。新增使用 `./scripts/new-task-worktree.sh <module> <task> --base <ref>`；根目录由 `git config --local --get oasis7.task-worktrees-root` 指定，未配置时采用脚本默认值。复用或用户指定的外部工作区，首次本地编译前先运行 `./scripts/cargo-dev.sh --prepare-target`。入口选择规则见开发规范。
- `python3 scripts/cargo-cache.py report --json` 按 native 及 target-triple 的 profile 列出 deps、incremental、build 的分配空间。日常关闭 incremental 不会删除历史状态；需要回收时用 `python3 scripts/cargo-incremental-clean.py --target <report 中的 target>` 预览，明确执行再加 `--apply`。仅接受有效托管标记；取得 wrapper、注册窗口及 Cargo 排他锁并检查进程占用后只删除 incremental，忙、未知归属、符号链接及不支持平台均保留。正式编译和验收参数不变。
- `resource-cleanup-executor.py` 移除工作区后输出对应外部缓存、默认 7 天保留期和 GC 预览命令；缓存报告失败不否认已完成的 Git 清理，也不自动删除缓存。`runtime-module-routing-perf-harness.sh --profile dev` 使用开发 wrapper，默认 release 性能测量保留原入口。
- 最小开发反馈示例：`./scripts/cargo-dev.sh check -p oasis7_proto`；`./scripts/cargo-dev.sh test -p oasis7_proto <test-filter>`；需要完整交互时才运行 `./scripts/worktree-harness.sh up`。不要用跳过必要验收来节省空间；正式范围仍由本手册与 CI 决定。
- `build-game-launcher-bundle.sh --profile dev` 的 native 编译与读取目录使用相同开发缓存；默认玩家 bundle 不编译 operator binaries，只有显式 `--ops-out-dir` 才编译它们。packaging 和 wasm 构建保留原入口。
- 本地 CI 验收和 launcher 打包入口会先准备上述链接；wasm Docker 构建会额外挂载已有链接的目标，容器宿主无需安装本地 Rust，首次构建前可手工准备链接，不修改正式 Cargo 参数或 profile；CI checkout 不建立链接。deterministic wasm / release 链路继续保持 `CARGO_TARGET_DIR` 为空；涉及 `scripts/build-wasm-module.sh`、release evidence 或 hash/receipt 对账时，不要改用 `scripts/cargo-dev.sh`。

### 本地 Cargo 缓存盘点与回收

先执行 `python3 scripts/cargo-cache.py report --json`。报告从外部缓存反向扫描，包含没有 `target` 链接的工作区缓存；`worktree-gc-report.sh --footprint` 同时提供该缓存清单。按物理目录去重，工作区本地 target 与外部缓存分别报告，不把链接再次相加。容量预算是回收目标，不是通过删除活跃缓存强制达到的限额。体积与剩余预算按盘点快照估算，其他并行构建可能改变实际占用；需要时重新盘点。

回收先预览，再显式执行。`--apply` 仅支持 Linux/macOS（Unix）；Windows 支持盘点、预览与编译锁，但拒绝实际回收，避免无法保证子进程继承锁时删除使用中的产物：

```bash
python3 scripts/cargo-cache.py gc --budget-gib 60 --min-age-days 7
python3 scripts/cargo-cache.py gc --budget-gib 60 --min-age-days 7 --apply
```

仅回收具有当前 wrapper 管理标记、所属工作区已不在 Git 注册表、没有其他 target 链接引用、超过保留期且能取得排他使用锁的缓存。仍注册（包括路径已缺失）、正在编译/运行、标记异常、身份未知和 legacy 缓存均保留。wrapper 更新最后使用时间并在 Cargo 及其运行子进程生命周期内持锁；`new-task-worktree.sh` 的 Git 注册与回收的重新检查/删除窗口使用同一仓库锁，避免固定路径重新挂接时被删除；手工 `git worktree add` 若与回收并发，应通过 `python3 scripts/cargo-cache.py attach -- git worktree add ...` 执行。worktree 清理后再次执行缓存回收，工作区清理本身不递归清空外部缓存。显式 target override 不纳入自动管理。首次报告中的历史缓存不因名称相似而自动取得删除资格。

低调试配置不会清除已有大体积产物；本地普通 `target` 也不会被自动搬移或删除。报告预算无法满足时列出剩余占用与保护原因，不把未知目录视为可释放空间。

## 当前实现分布（2026-02-18 基线）

### 应用主链（world + runtime + simulator + viewer 协议）
- 核心 crate：`crates/oasis7`
- 主要测试分布：
  - 运行时：`crates/oasis7_runtime/src/tests/*.rs`
  - 模拟器：`crates/oasis7/src/simulator/tests/*.rs`
  - LLM 行为：`crates/oasis7/src/simulator/llm_agent/tests_part2.rs`
  - Viewer live 服务：`crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs`（内置 `#[cfg(test)]`）
  - 端到端集成：`crates/oasis7/tests/*.rs`

### Pixel World Bridge（Bevy + wasm）
- crate：`crates/pixel_world_bridge`
- 覆盖：
  - Bevy world projection、相机/选中态与渲染逻辑测试散布在 `src/*_tests.rs`
  - 确定性 raster 基线由 `scripts/viewer-pixel-world-bevy-pixel-regression.sh` 采集
  - `wasm32-unknown-unknown` 编译检查覆盖 WebGL2 bridge 的目标特定构建入口

### Viewer Web 前端（JS/HTML + wasm bundle）
- package：`crates/oasis7_viewer`
- 覆盖：
  - source 结构、feedback contract 与 SolidJS 组件测试由 package scripts 提供
  - Web 启动入口：`oasis7_game_launcher`（内置静态服务，`run-viewer-web.sh` 仅保留为兼容/排障工具）
  - Web 闭环采样：agent-browser CLI（详见 `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md`）

### 分布式与共识子系统
- Node：`crates/oasis7_node`
- Net：`crates/oasis7_net`
- Consensus：`crates/oasis7_consensus`
- DistFS：`crates/oasis7_distfs`
- 这些子系统有独立测试集，但当前 `scripts/ci-tests.sh` 只覆盖了其中一部分（见下文“CI 现状与缺口”）。

### 场景系统
- 场景定义：`crates/oasis7/src/simulator/scenario.rs`
- 场景矩阵设计：`doc/world-simulator/scenario/scenario-files.prd.md`
- 场景是 UI 闭环、协议闭环、压力回归的统一输入源。

## CI 现状与缺口（事实口径）

### 当前 CI/脚本已覆盖
- 入口 A：`scripts/ci-tests.sh`（主流程）
- `commit`：
  - `./scripts/doc-governance-check.sh`
  - `./scripts/check-script-executable-bits.sh`
  - `cargo test -p oasis7_consensus --lib`
  - `cargo test -p oasis7_distfs --lib`
  - `npm --prefix crates/oasis7_viewer run test:feedback-contract`
  - `npm --prefix crates/oasis7_viewer run test:ui`
  - 用途：显式本地诊断 baseline，不由普通 `pre-commit` 自动执行；不包含 `cargo test -p oasis7 --tests --features test_tier_required`，但包含 repo-owned Viewer Web contract + Solid 组件锚点回归。
  - 范围：该入口不调用 `rust_baseline`，因此不运行 Rust 文件体量检查或 fmt；需要这些检查时另行显式运行 `./scripts/ci-tests.sh required --group rust_baseline`（也会执行该组的其他检查）。
- `required`：
  - `./scripts/doc-governance-check.sh`
  - `./scripts/check-script-executable-bits.sh`
  - `./scripts/check-rust-file-size.sh`
  - `cargo fmt --check`
  - `cargo test -p oasis7 --tests --features test_tier_required`
    - 该 shard 现在只覆盖轻量核心基线；需要注册或执行 builtin wasm artifact 的 runtime 闭环用例已下放到 `test_tier_full`。
  - `cargo test -p oasis7_consensus --lib`
  - `cargo test -p oasis7_distfs --lib`
  - `npm --prefix crates/oasis7_viewer run test:feedback-contract`
  - `npm --prefix crates/oasis7_viewer run test:ui`
  - `npm --prefix crates/oasis7_viewer run build:software-safe`
  - 变更 `oasis7_game_client` 时，`workspace_support` 运行独立包的全部 targets strict Clippy 和库测试，并沿 Cargo 依赖图保留主应用消费者。CLI 可用 `cargo run -p oasis7_game_client --bin oasis7_game_client -- --help` 单独验证。
  - 变更 `oasis7_agent_api` 或 `oasis7_agent_harness` 时，planner 选择 `workspace_support` 内实际受影响的 package profiles：API wire-contract 集成测试、Harness 单元测试及对应 strict Clippy；Cargo 依赖图仍保留 `oasis7` 消费者组。Launcher 代码变更保留 native profile 与 `launcher_web`。支持包清单位于 `plan-rust-required-scope.py`；驱动以 `--support-plan` 接收 trusted selector 的完整 JSON，不将 JSON 插入 shell 源码。
  - profile 计划版本为 1；空、未知、重复 profile、未知版本或 full 计划漏项均拒绝。旧 trusted planner 没有 profile 版本字段时仍执行完整支持包库存，不提前缩测；控制/manifest/lockfile/未知输入及非包规则继续保守全量。每个 profile 的命令成功后才输出完成标记，任何失败退出仍由原 `required-gate` 聚合。直接本地 `required --group workspace_support` 不传计划时保留原完整入口。
  - planner 始终声明 `required_gate_baseline`：这是 required-gate 的普适治理/契约 authority，不等同于每个 Rust、Node、provider 或 P2P 套件都对所有路径执行。
  - 触达场景定义、场景 integration test 或场景 PRD 时，planner 追加 `scenario_regression`，运行窄命令：`env -u RUSTC_WRAPPER cargo test -p oasis7 --test oasis7_init_demo --features test_tier_full oasis7_init_demo_runs_`；不追加 test-harness 参数，保证 `CI_VERBOSE=1` 的 Cargo verbosity 仍由 wrapper 正确处理。
  - provider remote-HTTPS、P2P/testnet、state-sync、S10 与 release-preflight contract 只在 planner 选择 `operational_contracts` 时运行；对应跨平台 workflow jobs 未命中时直接 skip，保留 stable context 的同时避免 docs-only 或不相关 PR 分配 runner 执行跨域 operational work。
- `full`：
  - 调用 `run_full_required_superset`、`rust_baseline` 及下列 full 测试；函数名不代表当前已覆盖 `required` 的每个子检查。
  - 当前仍缺 `oasis7` / consensus / distfs / node / net / net-libp2p 的组级 Clippy 和 `viewer-auth-browser-security-smoke.mjs`；workspace-support 的 Clippy 仍会执行。`full-standard` 同样存在这些缺口，`full` 另加 `formal_process`。
  - `required` 全量包含于 full 是 [CI 分层设计](doc/testing/ci/ci-tiered-execution.design.md) 的目标，尚不能凭本入口通过宣称实现；应按变更范围执行实际选中的 required 组。
  - `cargo test -p oasis7 --tests --features test_tier_full,wasmtime,viewer_live_integration`
  - `cargo test -p oasis7_node --lib`
  - `cargo test -p oasis7_net --lib`
  - `cargo test -p oasis7_net --features libp2p --lib`
  - `./scripts/llm-baseline-fixture-smoke.sh`
  - 以 `wasmtime` 且不含 test-tier feature 构建真实 `oasis7_chain_runtime` / `oasis7_game_launcher`，由 `scripts/hosted-local-mock-production-guard.py` 读取 Cargo artifact 清单，分别核对 Hosted local-mock funding 参数被生产 guard 精确拒绝；不以其他启动错误代替拒绝结果。
  - `cargo test -p oasis7 --features test_tier_required,wasmtime --lib --bins`：启用跨 crate 测试夹具，保持未启用 `test_tier_full` 时的严格 Wasmtime 默认预算。驱动按相同 features 构建 Viewer，并通过 `PRE2_VIEWER_BINARY` 传入 Cargo 返回的真实路径；不依赖预先存在的 executable。
  - `run_full_required_superset` 显式调用当前 scoped-required capability；重叠 support shard 只保留一次。仍以实际命令为准，不能据函数名推断每个 required 子检查都覆盖。
  - 独立 `full-support` tier 仍运行 `cargo test -p oasis7 --features wasmtime --lib --bins`；`full` 和 `full-standard` 使用上述 `test_tier_required,wasmtime` 命令及对应 Viewer artifact。

`./scripts/ci-tests.sh` 不设默认 tier；省略参数会打印 usage 并失败，避免误把升级专用的 `full` 当成本地默认。

- 入口 B：`.github/workflows/rust.yml`
  - select 从 Git diff、base/head Cargo 依赖关系和小型目录映射选择真实验证组。
  - 轻量 baseline 始终执行；Rust、Web、平台、打包和 operational 组按适用范围并行。
  - `required-gate` 是最后汇总，明确依赖 select 和所有测试组；选中组必须真实成功，失败、取消、缺失及意外跳过均失败。
  - 控制逻辑或共享输入变化、未知影响及依赖解析失败扩大普通 required；迁移 PR 使用完整保留组，不由候选选择器缩小覆盖。
  - CI 和评审结果使用真实 HEAD、测试对象、Actions 运行和 review 记录，不依赖 Task、Project 或通用 receipt。
  - `full` 可显式本地运行；nightly full 保留广覆盖回归，专项环境仅在普通 CI 存在具体不足时使用。
  - 候选全量回归：`gh workflow run rust.yml --ref <candidate-branch-or-tag> -f run_mode=full`。所选 ref 必须包含该入口；非 main 手动运行执行 `candidate-full-regression`，缓存只读、不写 trusted sentinel，普通 PR 默认选择不变。main 手动 full 和 nightly 仍由原 trusted writer 执行。
  - 候选运行 summary 记录实际 commit 和 tree，并核对 checkout 与 event SHA 相等；它验证所选源版本，不自动代表最新 main 合并组合。检查该 job 实际成功，不能将其他跳过的 job 或旧运行算作全量通过。
- 入口 C：`.github/workflows/wasm-determinism-gate.yml`（构建 hash / receipt evidence 独立 gate）
  - GitHub-hosted runner 矩阵：`(m1|m4|m5) x (ubuntu-24.04/linux-x86_64)`
  - planner 先执行：`./scripts/plan-wasm-determinism-scope.sh --event-name <push|pull_request|workflow_dispatch> --base-ref <base> --head-ref <head>`
  - 仅命中的 module set 会实际执行：`./scripts/ci-m1-wasm-summary.sh --module-set <m1|m4|m5> --runner-label ... --out ...`
  - planner 校验后输出同源 `selected_module_sets_json`；collect/verify 在 job 层过滤 skip，并只展开选中模块。无关改动只运行 planner，单模块共三个 runner jobs，全部模块共七个。planner、collect 或 verify 失败均不能算通过。
  - Wasm 与 document-corpus workflow 按 workflow 和 PR 编号取消过期运行；非 PR 使用独立 run_id。只有后继匹配事件触发运行时才取消旧运行。
  - verify job 会按命中的 `module_set` 下载 summaries，并执行：`./scripts/wasm-release-evidence-report.sh --module-sets <m1|m4|m5> --skip-collect --summary-import-dir <downloaded-summary-dir> --expected-runners linux-x86_64`
  - verify job 同时上传 `summary.md/json + logs + module_sets.tsv` 的 release evidence report artifact
  - 若要补独立跨宿主产物证据，可把外部 Docker-capable macOS runner 产出的 summary 作为额外 import 输入，再以 `--expected-runners linux-x86_64,darwin-arm64` 做离线对账；该结果不属于 `test_tier_full`，也不替代 `./scripts/ci-tests.sh full`

### CI 工具缓存与 local-signer 验证

- Viewer 的 `viewer-js-required`、`viewer-performance-report` 只恢复当前 Cargo.lock 版本的 wasm-bindgen CLI 目录；key 包含 OS、架构、Rust 版本和两个安装 helper 的 hash，不使用宽松恢复。精确命中必须验证目录内 binary，损坏命中直接失败；冷目录以 `--ensure-cache --print-bin` 安装并验证。普通 PR 不保存该工具缓存。
- 现有 main schedule/manual-full `full-regression` 是唯一工具 writer，准备成功后立即保存，再运行 full tests。Rust 大缓存身份保持 `ci-full-regression-trusted-v2`。首次写入与后续自然 Viewer PR 的热命中需分别从日志核实；热路径应无 CLI 源码安装，缓存成功不代表 full 回归通过。
- Linux packaging 与 full 通过 discovery 运行 local-signer 完整 suite：`python3 scripts/ci-local-signer-tests.py`。只允许已知 Darwin descriptor 项 skip；零测试、缺 native 项、导入错误、其他 skip 或失败均返回非零。
- fleet-health 的 macOS 分片执行 `TMPDIR=/private/tmp python3 scripts/ci-local-signer-tests.py --require-darwin`，native descriptor 必须真实成功且整个 suite 不得 skip。测试目录与 local-file-signing runbook 选择 packaging/fleet；四个 Python 生产入口另选 workspace_support；未知生产模块继续 full。
- CI 控制改动的 PR 仍由可信 base selector 选择全部适用组。候选 selector 的精确路由仅在合入后成为可信规则，required-gate 的组集合与失败判定保留。
- 本地合同验证：`bash scripts/plan-wasm-determinism-scope.test.sh`、`bash scripts/ensure-wasm-bindgen-cli.test.sh`、`python3 scripts/ci-local-signer-tests.test.py`、`python3 scripts/plan-rust-required-scope.test.py`、`python3 scripts/ci-required-result.test.py`、`python3 scripts/ci-workflow.test.py`；前三项接入 workflow_governance。
- net 的 libp2p/Python/WASM 检查、Viewer auth smoke 与 performance visual build 均通过 `ci-tests.sh --group` 执行一次；YAML 负责工具、浏览器和依赖准备。stable 安装使用 minimal 并显式安装 rustfmt/clippy，nightly、rust-src 与 WASM targets 保留。

<a id="ci-formal-process-validation"></a>
### 真实进程分片与产物复用验证

要求与机制见[真实进程验收要求](doc/testing/ci/ci-tiered-execution.prd.md#ci-formal-process)和[调度设计](doc/testing/ci/ci-tiered-execution.design.md#ci-formal-process-design)。

- `./scripts/ci-tests.sh required`：普通检查，不隐式启动独立重型链路。
- `./scripts/ci-tests.sh required --group formal_process`：准备产物后顺序执行全部真实进程分片；兼容入口 `./scripts/formal-live-local-tests.sh "$PWD"` 调用相同清单和运行器。
- `./scripts/ci-tests.sh full`：full-standard 加重型验收。本地/CI 内部 `full-standard` 只执行普通与扩展检查；nightly/manual-full 另起共享产物矩阵，不执行两次重型链路。
- 合同回归：`python3 scripts/ci-formal-process.test.py`、`python3 scripts/plan-rust-required-scope.test.py`、`python3 scripts/ci-required-result.test.py`、`python3 scripts/ci-workflow.test.py`。使用 Python 3.11+；覆盖精确零匹配、清单遗漏/重复、旧基线扩大、选中后失败/取消/跳过、绝对期限和真实小型 libtest。
- hosted 验证检查构建与各消费 job 位于不同 Linux runner：下载同 tested SHA 的消费者 tar.gz、核对 manifest 与所选执行闭包、无本地 target 前置时运行全部真实用例。不得在分片调用 Cargo。普通组与正式矩阵分别依赖各自 producer，正式矩阵不等待普通构建；互斥正式 producer 恰有一个成功且另一个跳过，任一必选分片失败则固定汇总失败。
- 优化回归核对全部完整用例在逻辑 suite 与物理 shard 中恰好一次，保留长历史 70 次及健康集合 40 次连续操作。比较 producer、分包原始/压缩 bytes、下载、实际测试、重开/冷查/stop 各阶段；记录缓存来源、是否命中及 8 GiB 保存预算结果。fixture 和本地计数器只能证明对应机制，整体耗时改善须用实际 runner 结果另证。
- `output/formal-process/results.jsonl` 记录每个命令的开始时间、耗时、退出码、超时、测试数和清理状态。只上传此安全摘要；详细输出查看 job 日志，失败世界目录不自动上传。缺摘要、缺产物和构建失败须从 Actions 失败步骤诊断，不能计为通过。

<a id="ci-dependency-installation-validation"></a>
### CI 依赖安装与 Release 工具缓存核验

要求与设计见 [CI 分级执行 PRD](doc/testing/ci/ci-tiered-execution.prd.md#ci-install-trunk)和[依赖准备设计](doc/testing/ci/ci-tiered-execution.design.md#ci-install-trunk-design)。本节给出验证方法；具体执行版本、结果和未覆盖范围记录在对应 PR 与 Actions 运行中。

- 本地接线回归：`bash scripts/release-packages-trunk-cache-contract.test.sh`。检查 Release 使用计划源中的共享安装器、与 Rust CI/main writer 一致的归档身份、只恢复 action 和冷热均执行验证，必要失败不能变成成功。历史 tag 缺安装器时应在恢复前明确失败。
- 安装器行为：`bash scripts/install-ci-trunk.test.sh` 覆盖冷下载、合法缓存、损坏缓存/下载、错误版本、错误 pin 及受支持宿主的真实 binary。此测试首次需要下载固定官方归档并核验摘要；缺网络时如实记录未执行，不能以离线接线测试代替。损坏归档不得先解压或执行。
- hosted 功能：读取后续自然发生且已授权的 Release `build-web-dist`，确认工具验证后原有 Web 构建、source/BUILDINFO 与发布检查正常。不要为本节人为触发发布；未发生的发布保持待观察。
- 真实跨 tag 缓存：关联 main `full-regression` 的实际归档保存记录和两个不同 tag 的 reader，记录 run/job URL、event/ref、请求/匹配 key、工具/安装器身份、路径及 cache version 兼容条件、恢复和摘要验证结果。同 tag rerun、仅配置相同或只有 cache-hit 输出均不足以确认 main seed 复用。
- 冷暖量测：区分实际下载、跨 run 恢复、本地已有和恢复异常后下载；分别记录恢复、下载、验证安装、总准备及 writer 保存成本。日志只有 step 级时间时保持该粒度。附 runner image/OS/arch、source、Trunk 版本/摘要、helper 指纹、样本窗口与数量；失败和取消单列，排队和整个 Release 耗时另报。样本不足时报告原始观察，不预写提速百分比。
- apt 先观察索引更新、下载、安装、锁等待和失败分布。本次没有新增 apt helper；后续确需预算治理时，使用假命令与临时进程 fixture 验证正常/瞬态/永久错误、总预算、挂起或忽略终止信号的子进程、父进程先退出、取消与无受控遗留进程。不得在真实 apt/dpkg 上故障注入；强制终止后的包状态不明确时应失败并使用新 runner。

本地合同测试只证明其实际范围；main 合入、hosted 构建、跨 tag 恢复与性能收益分别核实，不新增通用 required check、预热工作流或验收回执。

### 当前 CI 未直接覆盖（需手册补齐）
- Web UI agent-browser 闭环（现为手动/agent 流程，不在 CI 默认路径中）。
- `m4/m5` builtin wasm hash 校验（`scripts/ci-tests.sh` 已移除 `sync-m4/m5 --check`）。
- runtime builtin wasm bootstrap / default-module / body-action 闭环（已从 required 下放到 `test_tier_full`）。
- Viewer headed / GPU 真环境性能在被选择时必须采集，但在稳定可复现的环境特定采样阈值、原始复现与有时限 waiver 生命周期建立前仍是 report/watch，不是 blocking required gate。

结论：
- `commit` 是默认本地提交基线，目标是尽快暴露格式/治理/viewer-support 回归，但不承担 `oasis7 --tests` required shard；
- ordinary PR 的 `required-gate` 是 impact-scoped premerge 最小 blocking set：先拦截改动影响面的缺陷，再在不降低充分度的前提下优化速度；基础 required 含 `oasis7 + consensus + distfs + viewer`，GitHub 可按 planner 追加 `node + net/libp2p` support shard；
- `full` 不是 ordinary PR 默认，只用于 release、高风险、历史缺陷升级、信号触发或 schedule 回归；planner 的 `scope=full` 仅表示 required tier 内 fail-closed 覆盖扩张，不等于选择 full tier；
- `required-gate` 已补充 changed-path scope planner；
- 测试选择及结果规则见 [开发流程规范](doc/engineering/workflow/source-of-truth.md)。
- `wasm-determinism-gate` 负责 `m1/m4/m5` hash / receipt evidence 独立 gate；
- 若目标是“整应用充分测试”，仍需在此基础上叠加 UI 闭环层（S6）与压力层（S8）。

<a id="testing-default-selection-tree"></a>

## 默认测试选择树（影响面覆盖 + 最小充分测试）

先按改动影响面选择最小测试集，再按风险升级；不要把 `required`、`full` 或 release gate 当作“无脑全跑”的替代品。每次结论至少写清：本次影响面、已跑最小测试集、证据路径、未覆盖/残余风险。

CI 分层口径：ordinary PR 以 impact-scoped `required-gate` 作为 premerge 最小 blocking set，缺陷拦截优先、速度优化其次；`full` 只因 release、高风险、历史缺陷升级、信号触发或 schedule 运行。任何被选择的性能 surface 都必须采集环境、原始复现与样本；稳定可复现的环境特定采样阈值及有时限 waiver 生命周期成熟前，结论仅为 report/watch。

1. 文档、治理、脚本元数据：默认 S0；docs-only 同时执行命中的 contract / planner 样例。若改动测试/发布口径，再追加对应脚本的 syntax/dry-run 或 planner 样例，不因文档改动直接升级到 full。
2. runtime / simulator / world-model：定向 S3 回归和 CI 选中的必要 Rust 组；按跨模块、持久化与确定性影响扩大。
   - runtime timing、`RuntimePerfSnapshot`、LLM latency split、health/bottleneck 或 report consumer 变更，先读 `doc/testing/performance/performance-coverage-gap-matrix-2026-06-09.md` 的 Runtime/LLM 行；`llm-longrun-stress.sh` 输出属于 S8 诊断/长跑证据，不是默认 PR latency gate。
3. Viewer / Web / 可见 UI：触达 `crates/oasis7_viewer/**` 时先跑 JS-required deterministic contract / component test；触达可见表面时追加 S6 JS-browser 截图与模型视觉评审。只有触达 `crates/pixel_world_bridge/**` 或真实 bridge Rust/wasm 构建依赖时，才把 S5 作为 scoped required。
4. node / net / consensus / distfs：对应 S4 子系统与消费者；在线协议、复制、恢复、finality 或编排语义相关时加 S9/S10。这些可以是普通 CI 的本地多进程测试；只有具体环境缺口才追加专项集成。
5. CI 控制：运行真实 Git fixture、候选执行入口、job 图与失败传播回归，再执行当前 HEAD 的普通 PR CI，遵守可信 BASE 计划选择的组。
6. builtin wasm / module release / hash：先跑对应 scope planner 与 module-set evidence；发布或跨 runner claim 才进入 release evidence 对账。
7. playability / player continue claim：自动化只能证明“没坏/可回归”；`L4A`、`L4B`、`L5` 按 claim 强度升级，不得互相替代。
8. release candidate：默认完整 release gate；任何 `--skip-*` 都必须写明原因，并在 summary 里保留 claim boundary。带 skip 的结果只能支撑剩余已执行步骤，不能支撑被跳过层级的 release claim。

### Rust required 范围预览

```bash
(
  set -e
  git fetch origin main
  scope_base="$(git rev-parse --verify 'FETCH_HEAD^{commit}')"
  scope_source="$(git rev-parse --verify 'HEAD^{commit}')"
  ./scripts/plan-rust-required-scope.sh --base-ref "$scope_base" --head-ref "$scope_source"
)
```

预览只分析已提交 source，不含未提交文件，也不表示测试已经执行。输出为 `groups`、`resources`、`reasons` 与 `scope`（`targeted` 或 `full`）；`full` 是 ordinary required 组全选。CI 从事件 BASE 同时读取 selector、`ci-required-scope.json` 和 result，使用 source 与 `--test-ref` 纳入实际合并组合；候选 checkout 的 `ci-tests.sh` 执行真实测试。普通组为原生 job，fleet-health 使用 Linux/Windows/macOS 矩阵。最终 BASE result 校验同一次 run 的原生 needs。路径选择与迁移由现有 Git fixture 回归验证。

同一 PR 的后续运行会替代同组已有运行；仅重跑当前 HEAD，旧 HEAD 重跑也会竞争。不同 PR 和非 PR run 独立。

## 分层模型（针对当前仓库）

### L0 静态与工件一致性层
- 目标：尽早拦截格式漂移、内置 wasm 工件漂移与基础脚本/文档契约问题；不承担目标特定编译。
- 性质：最快、最确定。

### L1 核心逻辑确定性层（oasis7 主体）
- 目标：覆盖 runtime/simulator/world-model/LLM 行为/viewer 协议主逻辑。
- 入口：`test_tier_required` 与 `test_tier_full`（主要在 `oasis7` crate）。
- 性质：主覆盖层，应承接绝大多数回归风险。

### L2 协议与联机集成层
- 目标：验证 viewer live、web bridge、离线回放链路、wasmtime 路径等跨模块协作。
- 性质：比 L1 慢，但比 UI 端到端稳定。

### L3 分布式子系统层（node/net/consensus/distfs）
- 目标：验证共识、网络、复制、存储一致性与恢复链路。
- 性质：不应缺席；否则“整应用测试”会有明显盲区。

### L4A synthetic 内部闭环层（Web 为默认）
- 目标：验证 formal player surface 的真实可用性（加载、交互、状态可见、无 console error），并在此基础上完成 synthetic internal playability review。
- 默认：agent / QA 在当前 git worktree 内做开发回归时，优先使用 `./scripts/worktree-harness.sh up` 起一套 worktree 隔离 Web 栈；它会为当前 worktree 派生独立端口组、bundle / runtime / artifact 根目录与浏览器 session，并把状态写到 `output/harness/<worktree_id>/state.json`。这一层属于 `L4A synthetic`，可以产出 UI 闭环、subagent review、persona panel 等内部模拟证据。`scripts/run-launcher-stack.sh` 保留为底层 bootstrap，并支持 `--bundle-dir <bundle>` 复用产物入口；当 bundle 缺少 freshness manifest 或已落后于当前工作区源码时，脚本会默认阻断。launcher stack 与 `--with-harness` 预热都走 formal gameplay 的 active LLM path；`--no-llm` 只保留给直接 `oasis7_viewer_live` 观战/调试排障。
- 完整 `L4` scaffold：`./scripts/prepare-playability-l4-review.sh` 只准备当前 worktree 自己的 `output/harness/<worktree_id>/artifacts/playability-l4-<timestamp>/`，包含 `l4-review-packet.md`、`role-review-cards/*.md`、`persona-cards/*.md`、`l4-summary.md`、`commands.sh`、`manifest.json`；生成命令和模板不代表任何层已执行。显式加 `--with-l4a-stack` 才先启动 harness，并在 `evidence/` 下冻结当前 `L4A` state / URL；该启动继承 formal gameplay 的 active LLM provider preflight，缺少 `OASIS7_LLM_MODEL` / 等价 `config.toml` 时会在 harness 启动前 fail-fast。离线 JSON、部分输出及重试规则见 [L4-C06](#l4-toolchain-validation)。
- 最低完成定义：至少回填 review packet、`producer_system_designer` / `qa_engineer` 角色卡，以及命中的其余角色卡与 persona cards；只跑 harness / S6，不回填这些卡片，不算 `L4A` 完成。
- 结论边界：这一层可以回答“synthetic 看起来会不会继续玩”，不能直接回答“真人是否真的想继续玩”。
- native 抓图：仅 fallback（Web 无法复现或 native 图形链路问题）。

### L4B 具身 agent 试玩层
- 目标：验证 agent 实际进入 formal player surface、执行真实操作链路后，是否表现出可持续继续游玩的行为与可解释的玩家杠杆判断；这一层的设计目标是尽可能逼近真人评审“还想不想继续玩”的判断效果。
- 默认目标：使用候选 release Viewer 产物，由 agent 经正式 UI 完成真实玩家动作、核对可归因世界结果，并根据结果作出下一步决策。URL 取自本次 harness/producer 的实际输出；登录、操作和采证绑定同一 owned browser session。独立试玩的输入、有限时长和结果合同沿用[自主试玩设计](doc/testing/governance/agent-runtime-self-playtest.design.md#6-接口与数据合同)。记录实际 `execution_mode`；固定脚本路线不能改名为自主规划证据。
- 构建与驱动前置：release 不安装 `__AW_TEST__`，即使 URL 带 `test_api=1` 也不安装。需要测试 API 的受控回归必须显式使用 `build:viewer:visual-test` 的隔离产物，并启用测试查询参数；它只证明所声明的测试范围，不能代签 release UI。先核对实际产物和驱动能力，能力不匹配时在登录/玩法等待前停止并留痕；不要为通过测试给 release 注入测试 API。
- 实际操作会话：默认 `hosted_public_join` 从 guest 识别开始，经正式登录/注册成为 authenticated；恢复会话也须核对当前 player 和 gameplay readiness 后再提交。准备者另开的 headed 窗口不能代签执行会话的登录。缺账号/配置、登录失败或 player 不一致分别记 blocker；不得自动签发 Hosted 身份或静默切换 `trusted_local_only`。凭据不进入日志、截图或卡片。
- 当前工具差距：在 `27db6f894bd106e9a4fee9ee3062cd8fd86e1056` 核对的 `./scripts/run-playability-l4b-agent.sh --l4-manifest <artifact>/manifest.json` 仍依赖测试 API、分开创建 producer/headed 与 automation session，并缺少实际操作会话的 Hosted 登录编排；其自动评分和早退结果也未满足下述合同。`commands.sh` 仍包含该旧入口，不能仅因生成成功就把它当作已验证的默认 L4B。[#4438 的关闭决定](https://github.com/nil-studio/oasis7/issues/4438#issuecomment-6127544475) 已在文档 PR #4641 承接后以 `not_planned` 停止独立跟踪；它仅为历史来源。L1 正式 UI/实际会话前置、L2 入口边界与 L3 scaffold 输出要求仍由现行合同保留，关闭不表示实现或验收完成；本次文档不增加尚未实现的 runner 参数。
- 与 `L4A` 的衔接：scaffold 还复制 `l4b-agent-playtest-card.md`、生成 `optional-internal-human-corroboration.md`。完整 `L4` 至少收口 `L4A` packet / cards、当前尝试的 `L4B` agent 卡和最终 `l4-summary.md`。已有启动部件可用于准备 release 环境；能否形成 L4B 以实际会话和下述证据为准，不以启动器名称判定。
- 执行结果：每次尝试独立保留 [RunResult](doc/testing/governance/agent-runtime-self-playtest.design.md#6-接口与数据合同)，summary 显式引用 `current_attempt`、结果和证据路径。沿用 `preparing -> ready -> playing -> verifying -> reported -> cleanup`，阻断、取消和中断也记录已到阶段、停止原因、清理结果及未覆盖项；缺前置是 `blocked`，未知是 `inconclusive`，已执行且违反约定才是 `fail`。新尝试不能借旧成功产物通过。旧 runner 未完成该写出合同前，使用新 artifact 并保留已评审卡片，缺 RunResult 明确写缺失。
- 最低完成定义：agent 实际游玩、具备当前尝试的动作/结果/后续决策证据，RunResult 与 `l4b-agent-playtest-card.md`（或等价正式卡片）已落盘，再由 `producer_system_designer` / `qa_engineer` 审阅 `L4B` verdict。只有启动、截图、时间/进度增长或 ACK，不足以填写 `agent_continue_observed`、玩家杠杆满分或 `world_activity_only=no`。五分项分别需要动作、可归因变化、结果解释、新决策与继续理由；缺观察保持未知。无实际试玩时 L4B 为 `missing`，不能把登录失败解释为 agent 不想玩。
- 评审归属：runner 只追加当前尝试的事实；自动产物和旧 runner 导入默认 `unreviewed`，不能覆盖已经评审的最终 verdict。`unreviewed` 是评审占位状态，与执行 verdict、`go/watch/hold/block` 分开；重评须绑定当前候选和尝试，并保留旧结论。
- 结论边界：这一层可以回答“agent 在真实操作链路里是否表现出继续玩的倾向”，并应尽量逼近真人评审效果，但仍不能自动等价于 `L5` 真实人类或外部市场验证。
- 可选内部真人佐证：制作人试玩 / QA headed rerun 仍可沿用 `./scripts/run-producer-playtest.sh`；如执行，必须把结果写入 `optional-internal-human-corroboration.md` 或等价正式卡片，并在 `l4-summary.md` 里明确它是 `L4B` corroboration / contradiction，而不是新层级。只有人类试玩而没有对 `L4B` 的对照说明，不算合格佐证。
- source-tree Viewer Web 开发态统一通过 `run-launcher-stack.sh`（通常由 `worktree-harness.sh` 或 `run-producer-playtest.sh` 调用）并走 freshness gate；当 `crates/oasis7_viewer/viewer.html`、`viewer.js`、`viewer_first_agent_claim_evidence.html`、`package.json`、`package-lock.json`、`vite.software-safe.config.mjs`、`scripts/`、`software_safe_src/` 或相关静态资源比 `dist/` 更新时，默认应优先重建 fresh dist。重建时由 canonical `viewer.html` 与 claim fixture 生成 `dist/software_safe.html`、`software_safe.js` 和 `software_safe_first_agent_claim_evidence.html`；freshness / dist copy contract 会验证这些 outputs。

<a id="l4-toolchain-validation"></a>

### L4 工具链合同验收

下表承接 [L4 工具链 PRD](doc/testing/prd.md#l4-build-driver)与 [testing design](doc/testing/design.md)。它是实现修复时的定向验收矩阵；文档链接/格式检查通过不表示这些运行场景已经执行。每个适用场景按[来源到场景记录](doc/testing/governance/agent-runtime-self-playtest.design.md#selftest-source-scene)绑定候选、实际产物、配置/会话、命令或 UI 动作、原始证据和逐项 verdict。Hosted、浏览器、真实 provider 及玩法采证只在对应执行任务授权的范围与预算内进行。

| 场景 | 操作与正/负路径 | 必须核对的结果 | 执行范围与证据边界 |
| --- | --- | --- | --- |
| L4-C01 构建与驱动 | 分别检查 canonical/compat 页面 × release/visual-test；release 加 `test_api=1`，测试产物分别启用和省略/关闭测试参数；用要求 API 的驱动连接 release | 两种入口保留兼容语义；release 始终无 API，测试产物仅显式启用时安装；驱动不匹配在登录/玩法等待前停止并报告实际 profile | 定向构建/浏览器能力检查；只证明该产物与驱动合同，不形成继续游玩结论 |
| L4-C02 Hosted 会话 | 在实际执行 session 测 clean guest、正式 UI 登录成功、已认证恢复、缺配置、登录失败和另一窗口已登录但执行 session 未登录；操作前比较 player | 仅同一实际 session 的 authenticated/player/readiness 可放行；失败分阶段留痕，不自动签发身份，不回退 trusted local | 使用获授权的测试账号/环境；W3/transport 诊断或另一会话不能替代 Hosted 产品链路，凭据不进入证据 |
| L4-C03 执行终态 | 对可创建 artifact 的启动、能力检查、登录、操作及采证失败设置受控故障；另测取消、中断、清理失败和“旧成功后新阻断” | 每次保留独立结果和停止阶段，`current_attempt` 指向新尝试；缺前置/未知/违约分别归类，清理结果不伪装玩法结果；旧汇总不覆盖当前阻断 | 先用受控 fixture 检查结果写出与指针，再用适用环境验证真实故障；若输出目录本身不可写，stderr/退出结果明确说明无法落盘，不能声称有结果文件 |
| L4-C04 事实与判断 | 提供只有 progress/ACK/player 一致的样本、完整五项观察样本、缺一项样本；对已评审卡片重跑事实采集 | 机械事实不自动产生继续结论、满分或非环境推进判断；五项逐项引用证据，缺项未知；自动采集不覆写已评审 verdict，固定脚本如实记录执行模式 | fixture 可检查写入归属；真实继续倾向、因果与理解必须来自实际会话和评审，不能由 fixture 代签 |
| L4-C05 兼容产物与操作入口 | 对照本手册、game-test、Viewer entry compatibility；在 L4-C01 四格中分别执行 alias 生成、dist copy 与 stale-dist rebuild，并复核 opt-in 正负例；不同 worktree 使用各自实际 URL/session | 所有兼容产物保留，复制/重建前后类型一致且不覆盖另一类输出；无固定端口、跨会话假定或 release 测试 API 等待；错误 URL/session 明确阻断，原锚点保留 | 文档链接检查、alias/copy/freshness 定向回归与适用浏览器验证；历史 URL 保留为历史证据，不当当前默认 |
| L4-C06 离线 scaffold | 不带 `--with-l4a-stack`，以普通、含空格、含单引号的合法新目录运行 `--json`；再用已存在目录重试，并模拟生成后失败 | 成功 stdout 为单个 JSON 对象且退出 0；`run_id/output_dir/packet_path/summary_path` 与 manifest 一致，`manifest_path` 指向它；已存在目录退出 2 且不改原文件，部分输出失败明确范围并非成功 | 纯离线产物验收，不启动游戏/模型。shell 到 Python 序列化及路径回归仍需实际实现与验收；#4438 仅为已关闭的历史来源，不能在本次文档验证中记为通过 |

L4-C05 保留已有三条回归入口：[alias 生成](crates/oasis7_viewer/scripts/viewer-compat-aliases.test.mjs)、[dist copy](scripts/copy-viewer-web-dist.test.sh)、[stale-dist fallback](scripts/agent-browser-viewer-dist-freshness-test.sh)。在实现验收中还须补齐上述两类构建的类型保持断言与实际入口采证，不能把现有 fixture 通过直接当作全部四格已覆盖。

Scaffold 操作语义：`--json` 是已有 CLI 选项，成功的五字段摘要应只占 stdout，诊断写 stderr。若命令非零退出，即使目录已有卡片或 manifest，也只算部分输出；保留现场、检查失败原因，并为新尝试选择新目录/`--run-id`，不要自动删除或覆盖旧证据。`commands.sh` 只记录待执行步骤，其存在和可执行位不证明 L4A/L4B 成功。

### L5 真实人类 / 受控线上验证层
- 目标：验证真实人类或受控外部玩家在真实时间、注意力和机会成本约束下，是否仍愿意继续玩；这是 `L4B` 之上的正式验证层。
- 入口：limited preview、liveops 反馈回流、真实玩家 session、受控人工试玩样本。
- 结论边界：只有到这一层，才能正式回答“真实人类/真实环境里是否仍想继续玩”；内部真人 spot-check 仍只算 `L4B` 的可选校准，不单列新层。
- 说明：长稳、压力和 soak 仍属于测试套件层（如 S8/S10），不是这里的 playability `L5`。

### Playability review、persona 与 claim 收口
- L4A packet 记录 change scope、target claim、formal surfaces、evidence、blockers、roles、personas、questions 与 target lane；card 必须区分直接证据、推断、未证明项和 follow-up。输入不足写 `insufficient_input`，只转述他人结论写 `secondary_review_only`。
- 默认先由 `producer_system_designer` 与 `qa_engineer` 审核，按 surface 追加 viewer/agent/runtime/WASM 角色；涉及 limited-preview 或外部 claim 时追加 `liveops_community`。角色 review 完整只构成 L4A。
- persona 仅按主观风险选择 `new_player_confused`、`impatient_action_player`、`systems_optimizer`、`narrative_curiosity_player`、`chaos_tester`，并回流标准角色。多 persona 同节点负面或发现 exploit/dominant strategy/softlock 时升级工程角色与 L4B；persona 不是正式角色，不能单独给 L4B、L5 或 stage verdict。
- `block`: formal surface 不稳定或没有稳定玩家杠杆；`hold`: L4A 未支持 continue 或 L4B 有高价值反证；`watch`: L4B 缺失/样本薄/与校准冲突；`go`: 仅当目标 claim envelope 内 L1-L3、需要的 L4A/L4B 无高价值反证且 L5 无新反证。producer 定义 claim envelope，QA 守证据边界。

## 测试套件目录（S0~S10）

### S0：基础门禁套件（L0）
```bash
./scripts/doc-governance-check.sh
./scripts/check-script-executable-bits.sh
./scripts/cargo-dev-lib.test.sh
# 可选真实跨路径缓存验证（需要 sccache 和本地已下载的 itoa）
./scripts/cargo-dev-sccache.test.sh --real
./scripts/check-rust-file-size.sh
env -u RUSTC_WRAPPER cargo fmt --all -- --check
```
- S0 是适用于任何改动的快速静态基线：不运行 Viewer/Bevy、wasm 或任何其他 target-specific 编译；这类验证必须按改动面进入 scoped required（例如 S5）。
- 本地日常迭代若只是为了更快得到开发反馈，默认把同类命令替换为 `./scripts/cargo-dev.sh check/test/run/build ...`；新 task worktree 的 ignored `target` symlink 只是降低误用直接 cargo 时的重复存储，不改变正式验收入口。但进入正式 required/full 验收、尤其是 deterministic wasm/release 相关链路时，仍以本手册列出的原始 cargo 命令为准。
- 本地脚本内的开发态 build/test/run 应通过 `scripts/cargo-dev-lib.sh` 复用 shared target；正式验收脚本、release workflow、deterministic wasm、module release acceptance、hash/receipt evidence 仍不得被该 helper 隐式改写。
- `./scripts/check-rust-file-size.sh` 现同时校验超限基线、`touch-and-shrink` 和 `split_part/include!` 结构切片基线，不再只是“有没有新 >1200 文件”。
- 可选（按需执行 builtin wasm hash 校验）：
```bash
./scripts/sync-m1-builtin-wasm-artifacts.sh --check
./scripts/sync-m4-builtin-wasm-artifacts.sh --check
./scripts/sync-m5-builtin-wasm-artifacts.sh --check
```
- 本地策略（2026-03-08 起）：
  - 主 CI 仅允许 `--check`；生产发布清单写入与激活由发布节点链上流水完成。
  - 本地非 `--check` 仅允许显式维护清单（需设置 `OASIS7_WASM_SYNC_WRITE_ALLOW=local-dev`），不属于生产发布路径。
  - `CI=true` 不再作为生产发布写入/激活授权条件；CI 产物仅用于开发回归和可审计对账证据。

### S0.5：本地提交 commit baseline（L0 + 轻量 support/viewer）
```bash
./scripts/ci-tests.sh commit
```
- 覆盖重点：
  - 文档治理、release 关键脚本执行位等 `baseline` 治理/契约检查；不含 `rust_baseline` 的 Rust 文件体量和 fmt 检查
  - `oasis7_consensus` / `oasis7_distfs` 轻量 support crate
  - `software_safe` feedback contract regression
- 边界：
  - 普通 `pre-commit` 是静默 no-op；这一层仅由操作者显式运行，不是自动提交门禁。
  - 这一层不包含 `cargo test -p oasis7 --tests --features test_tier_required`。
  - 这一层也不包含 `cargo test -p pixel_world_bridge --lib` 与 `cargo check -p pixel_world_bridge --target wasm32-unknown-unknown`；若改动命中 `pixel_world_bridge` 或真实 bridge Rust/wasm 构建链路，显式执行 S5。

### S1：核心 required 套件（L1）
```bash
./scripts/ci-tests.sh required
```

先用适用局部验证获得反馈，再核对 GitHub 当前版本的真实 required-gate 结果；S1 是显式本地诊断命令。
- 覆盖重点：
  - runtime/simulator 大量单元与集成测试
  - `oasis7_viewer_live` 二进制测试
  - viewer offline integration
  - 分布式基础子系统（轻量）：`oasis7_consensus`、`oasis7_distfs`
  - release 关键脚本执行位前置校验，避免只在 `release-gate-web` / `package-native` runner 上才暴露 `Permission denied`
  - full support-crate shard 覆盖 `pixel_world_bridge` lib tests；受影响时，`workspace_support` 还运行 agent API wire-contract 集成测试、agent harness 单元测试及两 crate 的 strict Clippy。目标特定 wasm/build 检查仍按 S5 scoped required 触发。
  - 适用场景：PR/CI required gate，以及本地 landing 前需要显式补跑 `oasis7 --tests` required shard 的改动

### S2：核心 full 套件（L1 + L2）
```bash
./scripts/ci-tests.sh full
```
- 相对 S1 增量：
  - `test_tier_full`
  - `wasmtime` 路径
  - `viewer_live_integration`
  - `oasis7_node --lib`、`oasis7_net --lib`
  - `oasis7_net` 的 `libp2p` 路径
  - `llm-baseline-fixture-smoke`（基线加载与离线治理续跑断言）

### S3：应用主链定向套件（L1 + L2）
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required runtime::tests:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required simulator::tests:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required viewer::live::tests:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required viewer::web_bridge::tests:: -- --nocapture
```
- 电价/市场机制定向回归（required/full）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required simulator::tests::power::power_buy_zero_price_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required simulator::tests::power::power_order_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_full simulator::tests::power:: -- --nocapture
```
- 主链 Token / NodePoints 桥接定向回归（required/full）：
```bash
./scripts/main-token-regression.sh required
./scripts/main-token-regression.sh full
```
- 当前运行与审计权威：`doc/p2p/token/mainchain-token-allocation-mechanism.prd.md`。
- 用途：
  - 快速定位 `oasis7` 内部模块回归，不必每次跑全套 full。

### S4：分布式子系统套件（L3）
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_node
env -u RUSTC_WRAPPER cargo test -p oasis7_distfs
env -u RUSTC_WRAPPER cargo test -p oasis7_consensus
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_net --features libp2p --lib
```
- 可选增强（涉及 runtime_bridge 改动时）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_net --features runtime_bridge --lib
```

<a name="s4b：governance-registry-drill（l4-前置-l4）"></a>

### S4B：Governance Registry Drill（L4 前置 / L4）
- 适用场景：
  - 治理 signer rotation / revocation / failover runbook 首轮验证
  - `MAINNET-2` / `BENCH-G1` 需要留下 `pass + block` 审计证据时
- 推荐入口：
```bash
./scripts/governance-registry-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id msig.foundation_ops.v1 \
  --replace-signer-id signer03 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality slot 示例：
```bash
./scripts/governance-registry-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer03 \
  --replacement-signer-id signer04 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- 统一持久大世界的 committed execution-state 正式证据入口：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer02 \
  --replacement-signer-id signer05 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality multi-signer loss / rejoin 示例：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer02 \
  --replacement-signer-id signer05 \
  --block-remove-signer-id signer01 \
  --block-remove-signer-id signer02 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality non-baseline rejoin 示例：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --replace-signer-id signer02 \
  --replacement-signer-id signer05 \
  --replacement-public-key <replacement_public_key_hex> \
  --out-dir output/governance-drills/<run_id>
```
- finality baseline rejoin 示例：
```bash
./scripts/governance-registry-live-drill.sh \
  --source-world-dir output/chain-runtime/viewer-live-node/reward-runtime-execution-world \
  --baseline-manifest /path/to/public_manifest.json \
  --slot-id governance.finality.v1 \
  --pass-manifest-mode baseline \
  --replace-signer-id signer02 \
  --out-dir output/governance-drills/<run_id>
```
- 产物约定：
  - `run_config.json`
  - `summary.json`
  - `summary.md`
  - `manifests/{rotated_pass_manifest.json,degraded_block_manifest.json}`
  - `logs/*`
  - live-world 额外包含 `world-backup-pre-drill/*`
- 判定口径：
  - baseline / pass case 应返回 `overall_status=ready_for_ops_drill`
  - negative block case 可能有两种合法阻断结果：
    - `audit_failover_gate`: `block_import_rc=0` 且 `overall_status=failover_blocked`
    - `import_policy_reject`: `block_import_rc!=0` 且后续对 block manifest 的审计表现为 `manifest_mismatch`
  - 若 `block_enforcement_stage=audit_failover_gate`，脚本还会继续产出 `rejoin_case`；其期望结果是 `overall_status=ready_for_ops_drill`
  - `pass_manifest_mode=baseline` 适用于 temporary offline / same-signer rejoin；`pass_manifest_mode=rotate` 适用于 replacement / revocation 恢复
  - clone-world 样本只证明 runbook/tooling 正确，不替代统一持久大世界 committed execution-state 的最终 QA 证据
  - `governance-registry-live-drill.sh` 会在真实默认 execution-state 上自动执行 `baseline -> pass -> block -> restore`
  - 当 block case 仍可导入时，`governance-registry-drill.sh` / `governance-registry-live-drill.sh` 会额外执行 `rejoin`
  - controller slot 可保持原 `signer_id` 仅替换公钥；`governance.finality.v1` 不行，必须显式传入新的 `--replacement-signer-id`
  - `--block-remove-signer-id` 可重复使用；当 block manifest 让 `finality signer_count < threshold` 时，默认预期是 `import_policy_reject`
  - 若对 finality slot 复用原 `signer_id`，真实导入会命中 `GovernancePolicyInvalid`，因为 finality signer 绑定到现有 node identity

### S5：Pixel World Bridge（Bevy）单测与 wasm 编译套件（L4A 前置）
```bash
env -u RUSTC_WRAPPER cargo test -p pixel_world_bridge --lib
env -u RUSTC_WRAPPER cargo check -p pixel_world_bridge --target wasm32-unknown-unknown
```
- 说明：
  - `pixel_world_bridge` 是当前 Bevy/WebGL2 pixel-world 目标；`oasis7_viewer` 是其 JS/HTML bundle 消费方，不是 Rust Viewer/Bevy crate；
  - 这是 Viewer/Bevy 或 pixel-world wasm build 改动的 scoped required 稳定性筛网，并且是 UI 闭环前的稳定性前置；
  - raster、真实浏览器与性能证据不由本套件替代，按 S6 的 JS-browser / JS-full 升级。

<a id="testing-s6-qa-receiver"></a>
<a id="testing-web-ui-closed-loop-smoke"></a>

### S6：Web UI 闭环 smoke 套件（L4A）
- JS-required（结构/反馈/Vitest/freshness/build）：触达 `crates/oasis7_viewer/**` 的 source、生成物边界或 bundle 时，执行 `npm --prefix crates/oasis7_viewer run test:frontend-structure`、`npm --prefix crates/oasis7_viewer run test:feedback-contract`、`npm --prefix crates/oasis7_viewer run test:ui`、`./scripts/agent-browser-viewer-dist-freshness-test.sh` 与 `./scripts/build-viewer-software-safe.sh`。
- JS-browser（真实浏览器）：WASM bundle ready 后，以真实 browser 验证关键交互、console、desktop 与 narrow viewport；任何 player-visible 改动必须通过此层，不能只以 JS-required 替代。
- JS-full（release/risk-triggered）：跨模式、恢复路径、长运行与性能验证；发布或风险触发时执行，包含适用的 raster/browser/performance 证据。
- S6 详细执行步骤、agent-browser 命令、发布门禁与补充约定已拆分到：
  - `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md`
  - `doc/testing/manual/web-ui-playwright-closure-manual.manual.md`（Playwright 实跑系列入口，管理真实本地栈 + 真实 UI 操作流程矩阵）
  - `doc/testing/manual/web-ui-agent-browser-closure-manual.prd.md`（需求边界/成功标准）
  - `doc/testing/manual/model-visual-review-sop-2026-05-29.manual.md`（截图加模型视觉评审，用于替代 routine 人工视觉 review）
  - `doc/testing/launcher/launcher-manual-test-checklist-2026-03-10.prd.md`（发布前人工体验与异常恢复检查清单）
- 本手册仅保留分层与触发矩阵，执行时按上述文档操作。
- S6 的跨层结果按 [QA receiver tier boundary](doc/testing/prd.md#qa-infrastructure-tier-boundaries) 记录。截图、console 与 `__AW_TEST__` 只构成 surface observation；提交/receipt/root 与 world effect 仍须用同候选权威 source/journal/API oracle 核对。`test_tier_full` 还需要真实 provider-backed Agent parity 与 external headed desktop+narrow evidence，未执行的 PWT 计划项保持 planned。
- S6 选择器：
  - UI 结构 / 文案 / DOM anchor：优先 `npm --prefix crates/oasis7_viewer run test:ui` 或对应 contract test。
  - 可见视觉 / 布局 / 遮挡 / 响应式：先采 desktop/mobile 截图，再执行模型视觉评审卡。
  - formal player surface 可用性：执行 headed/browser S6，证据必须包含截图、console、关键状态与操作结果。
  - playability claim：S6 只能作为 `L4A` 前置；继续游玩倾向需升级到 `L4A/L4B/L5` 对应证据。
- 模式总口径（`PRD-CORE-009`）：
  - `viewer` / `pure_api` 是当前正式玩家访问模式，分别对应默认 Web 入口和纯接口正式入口；`software_safe` 仅保留为 `viewer` 的兼容 alias。
  - `pure_api` 的正式游玩与 headed Web/UI 一样，默认要求 active LLM access；禁用 LLM 后只能做 blocked/observer-debug 诊断，不再计入正式可玩性证据。
  - `player_parity` / `headless_agent` 是 agent provider 的 execution lane；当前 Local Provider provider-backed 主口径必须写成 `agent_decision_source=provider_backed + agent_provider_backend=provider_local_bridge + agent_provider_contract=worldsim_provider_v1 + agent_provider_transport=loopback_http`，远程托管 bridge 则必须显式写成 `agent_provider_transport=remote_https`，`agent_direct_connect/provider_loopback_http` 只保留为兼容 alias，这些字段都不构成额外玩家访问模式。仅做本地 plumbing / UI 闭环时可使用 `provider_local_mock` deterministic loopback bridge；它不调用 LLM、不计费、不限额，但不能替代真实 LLM / parity / release playability evidence。
  - repo-owned `remote_https` 参考装配当前采用 `oasis7_provider_local_bridge` + `scripts/provider-remote-https/letai_provider_cli.py` + `nginx` HTTPS 反代；操作步骤见 `doc/world-runtime/runtime/provider-remote-https-bridge-operator-runbook.md`。
  - 任何 QA / release / playability 结论都应先标明玩家访问模式，再补充 execution lane；不得把 `headless_agent` 直接当成“第三种入口”。
- `oasis7_viewer_live` / Viewer 页面：默认使用 `agent-browser` 驱动页面与采集证据；当 `renderMode=viewer`（或兼容 alias `software_safe`）且带 viewer auth bootstrap 时，允许继续验证选中 Agent 的最小 `prompt/chat` 闭环。
- 需要验证真实玩家输入流程、真实 provider、真实本地栈的一整条 Playwright 闭环时，进入 `doc/testing/manual/web-ui-playwright-closure-manual.manual.md`。当前首个用例是 `PWT-001 Real Agent Chat`：`./scripts/viewer-real-agent-chat-regression.sh` 会启动本地 LetAI 栈、打开 Viewer、通过可见 UI 聊天输入框和发送按钮发消息、等待真实 Agent 回复，并断言不含 mock 标记；后续所有 Playwright 实跑用例按该手册的 `PWT-###` 矩阵扩展。
- 若只需要回归 `software_safe` 纯实时最小闭环（加载 -> 连接 -> 选择目标 -> 实时事件/语义摘要可见，且页面不再暴露回放控件），优先执行 `./scripts/viewer-software-safe-step-regression.sh`；该脚本不再主动调用 `__AW_TEST__.sendControl('step')`，而是等待 `logicalTime/eventSeq` 自然增长；若当前 runtime 被 `llm_required` 等 gameplay blocker 卡住，则要求页面显式暴露 blocker，而不是再用手动步进补推进。
- 若只想先确认 Web/UI automation tooling 本身没有漂移，而不想起完整 runtime/build，先执行 `./scripts/viewer-software-safe-step-regression-smoke.sh`；它会用临时 fixture 页面复用真 `agent-browser` 与 `viewer-software-safe-step-regression.sh` 验证最小浏览器链路和 summary/state 产物契约，但不替代正式 S6 证据。
- 若需要把 `software_safe` 的 prompt/chat/rollback/message-flow 做成独立 QA smoke，优先执行 `./scripts/viewer-software-safe-chat-regression.sh`；当脚本自举 source stack 并自动启用 `OASIS7_RUNTIME_AGENT_CHAT_ECHO=1` 时，若 QA echo 没有在 `chat ack` 后、无额外 `step/play` 的同一轮交互里进入消息流，会直接判为阻断失败；外部 URL 场景仍默认把 `agent_spoke` 缺失记为可追溯 warning，显式加 `--require-agent-spoke` 时再升级为阻断失败。
- Viewer 浏览器测试（`__AW_TEST__`、visual fixture 或性能快照注入）先执行 `npm --prefix crates/oasis7_viewer run build:viewer:visual-test`；本地测试启动器默认使用 `crates/oasis7_viewer/.viewer-test-dist`。Live 测试仍由 launcher 注入可信 WS 配置；外部 URL 必须来自显式测试部署。

- 若用户反馈“Viewer 发卡 / 掉帧”，优先执行 `./scripts/viewer-performance-probe.sh --profile smoke --min-fps 55 --max-frame-p95-ms 20 --max-long-task-count 0`。该链路使用 `crates/oasis7_viewer/scripts/viewer-performance-probe.mjs` + `agent-browser` 直接采集 `requestAnimationFrame` frame timings / FPS、`PerformanceObserver` long tasks（浏览器支持时）、navigation DOM readiness、DOM 规模与截图，并输出 `output/playwright/viewer-performance/<run-id>/summary.json` 与 `summary.md`。
- 若改动只触达 `software_safe` feedback 语义映射而不需要浏览器自举，优先执行 `npm --prefix crates/oasis7_viewer run test:feedback-contract`；该 deterministic contract regression 已纳入 `./scripts/ci-tests.sh required`。
- 若改动触达 `crates/oasis7_viewer/software_safe_src/**` 的结构、Prompt/Chat surface、主入口锚点或移动端分区导航，优先执行 `npm --prefix crates/oasis7_viewer run test:ui`；这套 Vitest + `@solidjs/testing-library` 回归用于验证 repo-owned `World / Targets / Command` 锚点、`Runtime Diagnostics` 降级面、`Agent Chat` 与 `Prompt Overrides` 的 DOM 可达性，不替代 S6 headed browser 证据。
- 只要改动触达可视化相关代码、样式、资源或可见输出，S6 截图采证后默认执行模型视觉评审；典型触发面包括 UI component / DOM / CSS / layout / responsive / canvas / WebGL / pixel-world renderer / visual DTO 映射 / screenshot fixture / 图片资产 / 可见状态文案。`verdict=pass` 且 `confidence=high` 可替代 routine 人工视觉 review；`verdict=watch/block/human_escalation`、`confidence=medium/low` 或对外 claim 影响必须升级人类 owner；首次 `watch` 即升级，不等待第二轮。各组合及证据要求统一按[视觉评审判定表](doc/testing/manual/model-visual-review-sop-2026-05-29.manual.md#visual-review-decision-contract)执行。输出格式使用 `doc/testing/templates/model-visual-review-card-template.md`；只有明确不影响任何可见 surface 的改动才能豁免，并需在任务日志或 PR evidence 写明理由。
- 若需要稳定触发一条标准 `AgentSpoke` 供消息流验收，在 source runtime 启动前显式设置 `OASIS7_RUNTIME_AGENT_CHAT_ECHO=1`；该开关仅用于 Viewer / QA 测试态，默认产品路径必须保持关闭。
- 若 Viewer 页面长期停在 `connecting` 且 `logicalTime=0`，显式测试产物可查看 `window.__AW_TEST__.getState().lastError`；release 使用可见错误和 console 证据，不等待测试 API。命中 `copy_deferred_lighting_id_pipeline` / `CONTEXT_LOST_WEBGL` 等 fatal 时，按图形环境门禁失败处理，不进入玩法结论。
<a id="testing-software-renderer-scope"></a>

- `headed` 不是充分条件：若 `browser_env.json` / WebGL renderer 显示 `SwiftShader` 或其他 software renderer，先核对实际 `renderMode`；仅在显式测试产物中通过 `window.__AW_TEST__.getState().renderMode` 读取，release 使用可见入口与诊断证据。
  - `renderMode=viewer`（或兼容 alias `software_safe`）：允许继续做最小 DOM 闭环验证（连接、选择目标、自然实时推进；若运行态被 blocker 卡住，则要求 blocker 文案显式可见）。同时保留实际 renderer 与目标范围；不据此证明 Bevy/WASM/GPU 场景渲染、完整 S6、producer 试玩或发布可玩性。
  - `renderMode` 既不是 `viewer` 也不是兼容 alias `software_safe`：仍按图形环境阻断处理；默认先使用 `--use-angle=gl,--ignore-gpu-blocklist` 固定硬件路径。
- **原 lane 有限继续、另开调试、实际切换 lane 分别记账**：允许的软件 DOM 采样只收窄本次观察范围，保持原访问模式、身份与 execution lane。若另开受控调试会话，原尝试及其 blocker 保留，新会话按其实际配置和证据单独记录；若确需改变访问模式或 execution lane，须在既有授权范围内显式操作，记录能力/玩家可见差异、实际配置和新证据归属。`fallback_reason` 只是诊断，不授予切换权限，也不允许自动降到 trusted-local 或把 `headless_agent` 增强样本重标为 `player_parity`。
- `oasis7_web_launcher` / launcher Web 控制面：默认优先使用 GUI Agent 驱动产品动作，再用 Web 页面做状态与字段校验；Canvas 直点仅作补充。若目标是 `L4A synthetic`，优先走 harness / Web UI 闭环；若目标是 `L4B embodied-agent`，按本手册 L4B 的 release 正式 UI、实际 Hosted 会话和当前尝试结果合同执行，验收见 [L4-C01..06](#l4-toolchain-validation)。旧 `run-playability-l4b-agent.sh` 仍有上述实现差距，不能因自动生成 summary 就判定 L4B 完成。内部人类 spot-check 可用 `./scripts/run-producer-playtest.sh --open-headed`，结果写进 `optional-internal-human-corroboration.md` 或等价正式卡片，作为 `L4B` 校准附录；如需手动控制 bundle，再使用 `<bundle>/run-game.sh` 或 `./scripts/run-launcher-stack.sh --bundle-dir <bundle>` 启动。
- agent / QA 若只是想在当前 worktree 内起一套隔离回归栈，优先执行 `./scripts/worktree-harness.sh up`，然后通过 `./scripts/worktree-harness.sh url` / `status --json` / `logs` 获取 URL 与状态；`run-launcher-stack.sh` 继续作为该 harness 的底层启动器，不应再被当作并行 worktree 回归的顶层主入口。
- 不要把 Viewer 页面专用的 `agent-browser` 操作步骤直接套用到 launcher 控制面动作执行上。
- 涉及 `Explorer / Transfer` 的闭环时，先准备可观测数据，再执行查询与字段断言；不得只以“页面打开了/接口返回 200”判定通过。
- 防误用约束：
  - `scripts/run-game-test-ab.sh` 仅用于自动化回归哨兵（TTFC/命中率/无进展窗口）；推荐与 `--bundle-dir <bundle>` 搭配做产物态 smoke，但仍不等价于“真实玩家长玩评测”。
  - `run-game-test-ab.sh --headless` 若命中 `SwiftShader` / software renderer，核对实际目标 surface 与 `renderMode`；确认 `viewer`（compat alias `software_safe`）DOM 路径可用时，只继续该最小闭环。目标若为 Bevy/WASM/WebGL 场景层或真实 GPU 性能，则保留环境阻断与缺失范围，软件 DOM 通过不能填成 GPU/full/release/producer 通过。`connectionStatus=connecting` 本身不能区分图形、构建与 gameplay 失败；Viewer Web 默认继续使用 headed 模式。
  - 发布前若只要求 agent 可执行闭环，至少补齐 `L4B` agent 试玩与卡片填写；若要对“真实人类是否想继续玩”下结论，必须进入 `L5` 的真人 / 受控线上样本，内部真人试玩只能作为 `L4B` 的可选校准。
- 若改动影响前期工业引导（`首个制成品 / 停机恢复 / 首座工厂单元`），必须补跑 `doc/playability_test_result/topics/industrial-onboarding-required-tier-cards-2026-03-15.md` 中对应卡片，并把结论回写正式 playability 卡。
  - 对外样张链路需使用 strict 语义门禁，不得以 `off` / `soft` 结果作为发布判定证据。
- 若需要为 `#46 PostOnboarding` 补无 UI / 非浏览器验证，执行 `./scripts/viewer-post-onboarding-headless-smoke.sh`。
  - 该脚本只验证 live TCP 协议、快照推进、控制完成 ack 与 runtime event feed；不替代 headed Web/UI 截图复核。
- 若需要直接以纯 API 客户端操作 live 会话，可使用 `cargo run -q -p oasis7_client_cli --bin oasis7_pure_api_client -- ...`。
  - 该链路属于 `pure_api` 玩家访问模式；若同时牵涉 Local Provider provider-backed 路径，应额外标注 `agent_decision_source + agent_provider_backend/contract/transport` 与实际 execution lane；`agent_direct_connect/provider_loopback_http` 只在兼容迁移说明里保留。
  - 推荐最小链路：
```bash
cargo run -q -p oasis7_client_cli --bin oasis7_pure_api_client -- --addr 127.0.0.1:5023 snapshot --player-gameplay-only
cargo run -q -p oasis7_client_cli --bin oasis7_pure_api_client -- --addr 127.0.0.1:5023 step --count 8 --events
cargo run -q -p oasis7_client_cli --bin oasis7_pure_api_client -- keygen
cargo run -q -p oasis7_client_cli --bin oasis7_pure_api_client -- --addr 127.0.0.1:5023 reconnect-sync --player-id player-1 --with-snapshot
```
  - chain-linked Viewer 的 `chat` 会协商 `canonical_agent_chat_v1`，在同一连接用非空 owner public key 恢复**已注册**会话，再本地签名 owner ReadView 并校验原请求、world/branch/generation 和当前替换目标。首次注册仍须合法执行 `register-session`；Hosted identity 仍须有效 registration grant，CLI 不创建 owner claim 或 grant；canonical owner 身份须已经由合法 canonical 注册建立，Viewer register-session 不能代替它。
  - canonical chat 只把匹配原 nonce、agent、player、intent sequence/tick 的 committed ACK 当作完成；pending、unknown、provider 回复和无关后台错误都不是 chain commit。legacy 非 chain chat 继续等待 AgentSpoke 或请求错误。
  - canonical 请求发送前保存在持久用户状态目录下的独立私密目录（0700）中的 pending 文件（0600，文件与目录 fsync，不含私钥）；默认使用绝对路径 `XDG_STATE_HOME/oasis7/canonical-chat`，未设置时使用绝对路径 `HOME/.local/state/oasis7/canonical-chat`，没有可用持久路径时拒绝发送，不回退系统临时目录，不修改共享 HOME/state base 的权限；检查已有祖先路径分量，拒绝符号链接、外国所有者和不安全可写父目录，可信 root 所有的 sticky 临时父目录可容纳隔离测试的 owner 私密目录（此分量检查不声明消除目录替换竞态）；断连、超时或拒绝会输出文件路径。沿用原 `--addr`，使用 `chat-retry --signed-request-file <path>` 恢复同一请求。它校验原签名和保存的 endpoint/full WorldIdentity，并只查询 context 以核对原 genesis，不刷新或重签 authority/nonce；committed ACK 后仍保留 pending 文件，直到请求的 snapshot 后续步骤和 JSON 输出写入/flush 全部成功才删除；后续步骤或输出失败会明确报告已提交并保留路径，可用同一原请求查询/重试，不能据此重新生成 nonce。旧 fence 被撤销或 reorg 后拒绝时，保留文件供核查，由 owner 明确选择新 intent，不能自动重签绕过。
  - pending 文件和 owner read 输出含私密目标文字，请只由 owner 保存/读取。CLI 信任所配置的 Viewer TCP endpoint，由 Viewer 验证 WorldService 服务签名；这不是传输加密保证。私密 pending 文件权限验证目前支持 Unix；其他平台 canonical 路径拒绝发送，legacy 路径保持可用。
  - 若要覆盖 legacy 非 chain runtime 的 `agent_chat` / `prompt_control`，需先 `keygen`，再携带 `--player-id` 与 `--private-key-hex` 走签名请求；在这些非 chain runtime 路径中，只要 LLM 不可用，`gameplay_action / agent_chat / prompt_control` 会直接返回 `llm_mode_required` 或 `llm_init_failed`，而 `step / play` 会返回 `ControlCompletionAck { status: Blocked, error_code, error_message }`。
- 若需要执行 pure API required/full 回归，优先运行 `./scripts/oasis7-pure-api-parity-smoke.sh`。
  - 该回归验证的是 `pure_api` 玩家访问模式在 active LLM access 下的正式可玩性，不等同于 Local Provider `headless_agent` 回归。
  - required-tier 推荐 bundle 口径：
```bash
./scripts/build-game-launcher-bundle.sh --out-dir output/release/game-launcher-local
./scripts/oasis7-pure-api-parity-smoke.sh --tier required --bundle-dir output/release/game-launcher-local --with-llm
```
  - full-tier 抽样：
```bash
./scripts/oasis7-pure-api-parity-smoke.sh --tier full --bundle-dir output/release/game-launcher-local --with-llm
```
  - 结果说明：
    当前脚本已覆盖 `player_gameplay`、正式 `gameplay_action` 推进、`reconnect-sync --with-snapshot` 恢复，以及 `FirstSessionLoop -> PostOnboarding -> establish_first_capability|stabilize_first_line_after_output|choose_midloop_path` 的 required/full 收口路径。
    `parity_verified` 当前以 `doc/testing/evidence/pure-api-shared-player-gameplay-parity-2026-04-28.md` 为准；旧 `doc/testing/evidence/pure-api-parity-validation-2026-03-19.md` 只保留历史输入，不再作为 no-LLM 正式可玩性的现行依据。
- 本地试玩入口瘦身口径：普通操作者只需要按意图记住 `2 + 1` 个入口；底层脚本仍保留给 wrapper、排障和专项回归使用。
  - 入口语义先分流：

| 意图 | 入口 | 连接的 world state | 不能声明 |
| --- | --- | --- | --- |
| 纯本地试玩 / local-only gameplay | `./scripts/run-local-letai-game-test.sh --local-world-playtest` | local-standalone-chain | formal/public testnet connected |
| 本地接入 public_testnet 测试环境 | `./scripts/run-local-public-testnet-letai-test-environment.sh` | formal `public_testnet` node + submit-capable endpoint | 仅因页面打开就声明 public_testnet ready |

  - 本地真实 LetAI bridge + runtime/game：
```bash
./scripts/run-local-letai-game-test.sh --local-world-playtest
```
    常规本地真实 LLM 游戏测试统一从 `scripts/run-local-letai-game-test.sh --local-world-playtest` 启动；这个入口只代表完全本地的 provider/runtime/viewer/local-standalone-chain 试玩，不连接 formal/public testnet。若目标是“本地入口读取/提交 formal `public_testnet` world state”，不要使用这个 preset，改走 `scripts/run-local-public-testnet-letai-test-environment.sh` 和对应 runbook。每次纯本地试玩都应优先使用这个 preset，而不是手工分别启动 provider bridge / launcher / viewer binary，或展开端口、detach、reuse、provider smoke、chain profile 等低层参数。该 preset 固化 playtest startup、provider smoke skip、reuse existing source build、后台启动、默认自动 Play、viewer/web/live 标准端口 `48420/48421/48422`、`--json-ready` 与 wrapper 默认的 local standalone chain。脚本仍默认优先读取 `OASIS7_LETAI_CONFIG_PATH`，再使用 `/Users/scc/Documents/keys/letai.txt`，最后回退到 `OASIS7_LETAI_TOKEN_CONFIG_PATH` 或 `/Users/scc/Documents/keys/letai-token-local.txt`，设置本地代理默认值，规范化临时 project token config，转发 `--auto-topup-usd`、`OASIS7_REMOTE_LLM_PLATFORM_KEY`、`OASIS7_REMOTE_LLM_PLATFORM_USER_ID` 等真实 provider 必需环境，后台启动默认 `rust-direct-letai` 的 `127.0.0.1:5841` Rust provider bridge，再启动 `run-launcher-stack.sh` 指向该 bridge；额外 launcher 参数仅在高级排障时放在 `--` 之后。该 wrapper 默认保持 `OASIS7_RUNTIME_AGENT_CHAT_ECHO` 关闭，使本地 playtest 遵循真实 provider-backed 路径；只有做低层 receipt/debug 验证时才显式传 `--chat-echo`。若直接跑已编译二进制或手工拼装脚本，操作者必须自行保证这些 env 与 wrapper 完全等价，否则可能出现不自动充值、token/project 绑定不一致、或 mock/receipt 路径误入本地试玩证据。需要打开后手动 Play 时，执行 `./scripts/run-local-letai-game-test.sh --local-world-playtest --no-auto-play`；该选项只保持世界暂停，不表示跳过配置预检、provider/网络请求或停止后台栈。需要查完整调试参数时，用 `./scripts/run-local-letai-game-test.sh --help-all`。
  - 制作人 / 发布前人工验收：
```bash
./scripts/run-producer-playtest.sh --open-headed
```
    这是 bundle-first 人工试玩入口。脚本会自动准备或复用 fresh bundle，再打开 headed browser；若只想起栈并手动复制 URL，可省略 `--open-headed`。
  - QA / subagent evidence：
```bash
./scripts/worktree-harness.sh up
./scripts/worktree-harness.sh status --json
GAME_URL="$(./scripts/worktree-harness.sh url)"
./scripts/run-game-test-ab.sh --url "$GAME_URL"
```
    `worktree-harness.sh` 是当前 worktree 的隔离 L4A synthetic 栈入口；`run-game-test-ab.sh` 是 TTFC / 控制命中率 / 无进展窗口哨兵，不替代真人试玩或制作人验收。
  - 高级 / 底层入口（仅用于 wrapper、排障或专项回归，不作为普通试玩菜单）：
```bash
./scripts/build-game-launcher-bundle.sh --out-dir output/release/game-launcher-local
./scripts/check-active-llm-provider.sh --pretty
./scripts/run-launcher-stack.sh --bundle-dir output/release/game-launcher-local --with-llm
./scripts/viewer-post-onboarding-qa.sh --bundle-dir output/release/game-launcher-local --with-llm
./scripts/viewer-post-onboarding-headless-smoke.sh --bundle-dir output/release/game-launcher-local --with-llm
./scripts/viewer-software-safe-chat-regression.sh --bundle-dir output/release/game-launcher-local
cargo run -q -p oasis7_client_cli --bin oasis7_pure_api_client -- --addr 127.0.0.1:5023 snapshot --player-gameplay-only
./scripts/oasis7-pure-api-parity-smoke.sh --tier required --bundle-dir output/release/game-launcher-local --with-llm
```
  - active-LLM / provider 预检：
    `run-launcher-stack.sh` 仍是底层 launcher/runtime bootstrap。它会在启动 launcher 前先跑一次 active LLM provider probe，复用同一套 `config.toml` / `OASIS7_LLM_*` 配置，并同时验证 Responses hello 文本响应与 required tool-call 合约；若 provider/model/auth/base URL 当前不可用，或模型能回文本但不能稳定返回 tool call，会直接 fail-fast，不再等到首个 `step` 才暴露。需要故意保留“stack 可启动但 formal lane 在首步 blocked”的负向验证时，显式加 `--skip-llm-provider-preflight`。
    若本地只验证 provider-backed plumbing，可先启动 `oasis7_provider_local_bridge --mode mock`，再用 `run-launcher-stack.sh --agent-provider-lane local-mock` 指向 deterministic `provider_local_mock`。
    若本地需要 builtin LLM 直连验证，`scripts/with-letai-llm-config.sh` 只适用于已经包含 chat-completions/Responses-compatible `token_key` / `api_key` 的配置；不要把只有 `Doc` 和平台管理 `Key` 的文件直接当 inference token 使用。platform-only 配置应先通过 `scripts/ensure-letai-local-token-config.sh` 或 `scripts/run-local-letai-game-test.sh` 规范化成临时 project token config；不得把 raw key、生成的 `letai-local-token.env`、或其它 token config 写入日志、仓库文件或 public evidence。
    LetAI provider bridge 参考装配走 `/chat/completions`，与 builtin LLM 的 Responses preflight 不是同一协议；`scripts/run-local-letai-provider-bridge.sh` 是 `run-local-letai-game-test.sh` 使用的底层 bridge 入口，也可用于专项排障，随后用 `provider-bridge-contract-smoke.sh` 验证真实 provider decision。普通本地试玩不要从这个底层入口开始；应回到 `run-local-letai-game-test.sh`，让 wrapper 统一处理 token config、auto-topup 和 launcher 参数。`scripts/check-letai-chat-completions.sh` 和 `--chat-probe-backend legacy-cli` 只保留给兼容排障或对比 Python legacy adapter，不作为普通本地试玩默认 gate。
    本地真实 Rust provider bridge 默认在上游返回 `insufficient_user_quota` 且同时存在 `OASIS7_REMOTE_LLM_PLATFORM_KEY` / `OASIS7_REMOTE_LLM_PLATFORM_USER_ID` 时按 `$0.1`（`quota=50000`）自动充值；这是 paid real-provider 行为。充值触发按 chat request 发生，随后使用有限次数延迟重试来等待 LetAI 余额更新可见，而不是全局每次运行只充值一次；没有这些管理面字段时不会自动充值，仍应把 quota error 作为失败证据。

### S7：场景矩阵回归套件（L1 + L4）
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required scenario_specs_match_ids -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required scenarios_are_stable -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_full oasis7_init_demo_runs_ -- --nocapture
```
- 配套文档：`doc/world-simulator/scenario/scenario-files.prd.md` 的“场景测试覆盖矩阵”。

### S6.5：Chain Runtime Storage Profile / Gate 技术核验
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime node_runtime_execution_driver_uses_storage_profile_checkpoint_interval -- --nocapture
./scripts/oasis7-runtime-storage-gate.sh --status-json <status.json> --expected-profile release_default --min-checkpoint-count 1 --max-orphan-blob-count 0 --require-no-degraded
OASIS7_CHAIN_STORAGE_PROFILE=release_default bash -x <bundle>/run-game.sh --help
OASIS7_CHAIN_STORAGE_PROFILE=soak_forensics bash -x <bundle>/run-web-launcher.sh --help
OASIS7_CHAIN_STORAGE_PROFILE=dev_local bash -x <bundle>/run-chain-runtime.sh --help
```
- 说明：
  - 用于 `TASK-WORLD_RUNTIME-033` 的 storage profile / storage gate / bundle wrapper 一致性核验；
  - 若验证真实 `release_default` cadence，优先对比 `<64` 与 `>=64` 两个采样点，确认 `full_log_only -> checkpoint_plus_log` 切换；
  - 若验证 bundle 入口，只需保留 `bash -x` trace 作为“wrapper 实际注入了正确 profile 参数”的证据；
  - 参考证据：
    - `doc/world-runtime/evidence/runtime-storage-gate-sample-2026-03-10.md`
    - `doc/world-runtime/evidence/runtime-sidecar-orphan-gc-failsafe-2026-03-11.md`
    - `doc/world-runtime/evidence/runtime-launcher-profile-consistency-2026-03-11.md`

<a id="storage-profile-retention-density-validation"></a>
#### S6.5.1：release_default 保留密度配置断言

此步承接存储专题 [SC-7 配置子断言](doc/world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-case-sc7)，由 storage profile / QA 在实际候选核验。真实测试定义位于 [storage_profile.rs](crates/oasis7_proto/src/storage_profile.rs) 的 `storage_profile::tests::storage_profiles_scale_retention_density`，当前断言 `release_default.execution_hot_head_heights == 64` 且等于 `execution_checkpoint_interval`；64 是当前 source 断言，不把专业合同的 ≤ 条件改为新的固定数值要求。

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_proto --lib storage_profile::tests::storage_profiles_scale_retention_density -- --exact --nocapture
```

- 执行时固定 source/tested tree、命令及环境，保存完整 test 输出与退出码；必须实际运行恰好 1 个所列测试并通过、命令 exit=0。过滤后 0 tests、跳过、ignored 或仅编译成功均不能记为通过。
- 该步骤只证明当前 profile 配置的局部断言；不证明实际 retention 窗口、exact-height restore、未重复保留同区间 snapshot、2500 heights footprint 或恢复/服务组合。SC-7 原完整目标、R0–R3、required/full 与同候选验证义务继续独立，不由此步骤替代。
- 本次仅补准确验证方法，未运行上述命令，也未运行节点、GC、恢复或预算验收。driver_storage_profile 的 policy/cache/CAS 集成测试不是该配置 equality 的替代证据。

### S8：长稳与压力技术套件
- Viewer 当前 Web 性能 probe（当前活跃入口）：
```bash
npm --prefix crates/oasis7_viewer run build:viewer:visual-test
./scripts/viewer-performance-probe.sh --profile smoke
./scripts/viewer-performance-probe.sh --profile release --duration-ms 8000
```
- Runtime 轻量 deterministic perf harness（当前先覆盖稳定内环 module routing）：
```bash
./scripts/runtime-module-routing-perf-harness.sh
```
- LLM 长稳：
```bash
./scripts/llm-longrun-stress.sh --with-local-letai-provider-bridge --scenario llm_bootstrap --ticks 240
```
- LLM 覆盖门禁（发行口径）：
```bash
./scripts/llm-longrun-stress.sh --with-local-letai-provider-bridge --scenario llm_bootstrap --ticks 240 --release-gate --release-gate-profile hybrid
```
- LLM gameplay 对照（bridge 开/关）：
```bash
./scripts/llm-longrun-stress.sh --with-local-letai-provider-bridge --scenario llm_bootstrap --ticks 240 --prompt-pack story_balanced --runtime-gameplay-bridge --out-dir .tmp/llm_stress/<comparison-id>/bridge-on
./scripts/llm-longrun-stress.sh --with-local-letai-provider-bridge --scenario llm_bootstrap --ticks 240 --prompt-pack story_balanced --no-runtime-gameplay-bridge --out-dir .tmp/llm_stress/<comparison-id>/bridge-off
```
  将 `<comparison-id>` 替换为本次唯一标识，分别登记两侧候选版本、完整参数、wrapper 最终退出码/日志和原始工件路径。stress 会清理指定 out-dir；已有对照树后不得再用默认 `.tmp/llm_stress` 根目录命令，`--keep-out-dir` 也不避免同名文件覆盖。wrapper 日志放在其清理目录之外，按[Web/LLM 证据模板](doc/world-simulator/prd/acceptance/web-llm-evidence-template.md)判定；summary 存在不代表 gate 通过。
- git 跟踪基线 fixture smoke（`test_tier_full`）：
```bash
./scripts/llm-baseline-fixture-smoke.sh
```
- Prompt 切换覆盖对比（定向排障）：
```bash
./scripts/llm-switch-coverage-diff.sh --log <run.log> --switch-tick 24
```
- 说明：
  - 详细参数与 profile 组合请以 `./scripts/llm-longrun-stress.sh --help` 为准；
  - `llm-longrun-stress.sh` 是 provider-backed LLM 长跑入口；`--no-llm-io` 只关闭 raw LLM I/O 日志，不会禁用 provider。日常本机 LetAI 长跑优先使用 `--with-local-letai-provider-bridge`，让脚本生成临时 token config、继承本地代理并启动本地 Rust provider bridge，保留 auto-topup；若确实要复用已启动 bridge，必须显式传 `--reuse-local-provider-bridge` 接受既有 bridge 的 config/token 状态。`--with-letai-config` 只保留给直连 Responses API 的低层排障；
  - 多场景评测固定 scenario/fixture、profile、provider/adapter 与协议版本、timeout、tick budget 和 `--jobs`，并保留每场景及聚合工件；单次样本、总量或并行运行本身都不证明 provider parity、成本稳定性或 release readiness。历史多场景/工业调试样本仅供追溯，当前语义以 `doc/world-simulator/llm/decision-provider-contract.prd.md` 和相应 gameplay/runtime authority 为准；
  - `./scripts/runtime-module-routing-perf-harness.sh` 现在走专用 bin `cargo run -p oasis7_ops_tools --bin oasis7_runtime_module_routing_perf`，避免继续依赖 ignored test 的重编译路径；它会把稳定内环 module routing 的 event/action 平均耗时写到 `.tmp/runtime_module_routing_perf/<run>/summary.{json,md}`；当前目的是提供日常可回归的轻量 runtime perf 入口，不替代长窗 `runtime_perf.tick.*` 指标；
  - 当前已记录首个 `release` baseline 数值：`modules=192`、`iterations=80`、`event_avg_ms=5.591`、`action_avg_ms=6.992`；对应一次本地验证 run 的冷 `release` 编译耗时约 `23m 10s`。原始 `summary.json` 属于本地临时产物，应以 task execution log 中登记的验证记录为证据来源；因此当前更适合先作为本地/report-only 基线，而不是立刻升成默认 blocking gate；
  - Viewer 性能 probe 使用当前 `crates/oasis7_viewer` canonical `viewer` Web 入口，通过 `agent-browser` 采集 rAF frame timing、Long Task、ready time、DOM 规模与 gate 结果，并输出 `output/playwright/viewer-performance/<run>/summary.{json,md}`；
  - 旧 `viewer-owr4-stress` 属于历史已删除入口，不再作为当前 Viewer 性能门禁真值；
  - `scripts/ci-tests.sh full` 已接入 `./scripts/llm-baseline-fixture-smoke.sh`；
  - 压测结果需保留 CSV/summary/log 产物。

<a id="testing-p2p-storage-consensus-long-running"></a>

### S9：P2P/存储/共识在线长跑技术套件
- 当前状态（2026-02-28）：`scripts/p2p-longrun-soak.sh` 已恢复为可执行脚本，底座为多进程 `oasis7_chain_runtime`。
- 时间语义说明：PoS 出块/提案节拍由 `--pos-slot-duration-ms` 与 `--pos-ticks-per-slot` 锚定；`--node-tick-ms` 仅表示 worker 轮询/回退间隔。
- 建议命令（smoke）：
```bash
./scripts/p2p-longrun-soak.sh --profile soak_smoke --topologies triad --duration-secs 600 --no-prewarm
```
- 建议命令（endurance + chaos）：
```bash
./scripts/p2p-longrun-soak.sh --profile soak_endurance --topologies triad_distributed --chaos-continuous-enable --chaos-continuous-interval-secs 30 --chaos-continuous-max-events 60
```
- 建议命令（endurance + chaos + feedback）：
```bash
./scripts/p2p-longrun-soak.sh --profile soak_endurance --topologies triad_distributed --duration-secs 900 --chaos-continuous-enable --chaos-continuous-interval-secs 30 --chaos-continuous-max-events 30 --feedback-events-enable --feedback-events-start-sec 30 --feedback-events-interval-secs 60 --feedback-events-max-events 12
```
- 发布门禁基线命令（2026-02-28，300s）：
```bash
./scripts/p2p-longrun-soak.sh --profile soak_release --topologies triad_distributed --duration-secs 300 --no-prewarm --max-stall-secs 240 --max-lag-p95 50 --max-distfs-failure-ratio 0.1 --chaos-continuous-enable --chaos-continuous-interval-secs 30 --chaos-continuous-start-sec 30 --chaos-continuous-max-events 8 --chaos-continuous-actions restart,pause --chaos-continuous-seed 1772284566 --chaos-continuous-restart-down-secs 1 --chaos-continuous-pause-duration-secs 2 --out-dir .tmp/release_gate_p2p
```
- 通过标准：
  - 命令返回 `rc=0`；
  - `summary.json` 中 `overall_status == "ok"` 且 `totals.topology_failed_count == 0`；
  - `soak_release` 档位下 `topologies[].metric_gate.status` 必须为 `pass`（`insufficient_data` 会转失败）；
  - `topologies[].metrics.consensus_hash_consistent` 必须为 `true`，且 `consensus_hash_mismatch_count == 0`（若失败需检查 `topology/.consensus_hash_mismatch.tsv`）；
  - 如启用 chaos，`chaos_events.log` 与 `summary.json.totals.chaos_events_total` 一致。
  - 如启用 feedback events，`summary.json.totals.feedback_events_total == summary.json.totals.feedback_events_success_total + summary.json.totals.feedback_events_failed_total`，且 `feedback_events.log` 中 `phase=completed/failed` 事件数量与 `feedback_events_total` 一致。
- reward-runtime 目标合同（尚未全部由脚本保证）：S9/S10 只消费 `oasis7_chain_runtime` 的
  `/v1/chain/status.reward_runtime` 原生快照，禁止以兼容字段回推。每个可用节点
  先取累计最大值再聚合；缺失节点必须留告警，status 不可达记为 HTTP failure 并
  终止门禁判定。`distfs_total_checks=0` 为 `insufficient_data`，不得与比例超阈值
  混写；仅 `reward_runtime.invariant_ok=false` 可触发
  `reward_asset_invariant_violation`，`running_false/http_failure` 必须保留为独立
  失败分类，即使发生于 chaos 恢复窗口。
- 当前实现差距：两脚本的 mint 非空采样取 balances 的 `reward_mint_record_count` 与原生累计计数的最大值；节点累计聚合使用缺项默认 0，全局 available-sample 计数不能证明每个节点均有数据。S9 在存在 available samples 时把零 DistFS checks 记为 `distfs_checks_zero` warning，其他条件满足可得 `pass`；S10 则将零 checks 记为 data warning / `insufficient_data`。两者 status HTTP 失败后仍以默认字段继续采样，未立即终止；S9 的 HTTP/running 失败在 chaos 下可降为 warning，S10 HTTP 失败通常为 warning（全部 status 缺失或 running=false 等另可失败）。timeline 的 `invariant_ok` 来自 running/last_error，并非原生资产不变量；原生字段读取中的 jq `// true` 会把显式 false 也变成 true。因此脚本绿灯不能代签上述原生来源、逐节点完整性与独立失败分类合同。应核对原始 status、balances、缺失节点及失败样本，未证明项保持未满足；本说明不降低目标、不修改阈值，也不表示本次已执行或修复脚本。
- 漂移定位/回滚演练门禁（TASK-GAME-014）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required runtime::tests::persistence::rollback_with_reconciliation_recovers_from_detected_tick_consensus_drift -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required runtime_authoritative_recovery_rollback -- --nocapture
```
- 演练通过标准：
  - 能定位 `mismatch_tick`；
  - `rollback_to_snapshot_with_reconciliation` 后 `first_tick_consensus_drift() == None`；
  - `verify_tick_consensus_chain()` 通过。
  - Viewer 缺少签名信封返回 `rollback_approval_required`；篡改、过期、目标不匹配或重放信封返回 `rollback_authorization_invalid`，且两类失败均不得改变 world、journal、batch 或 reorg epoch。
- 参考文档：`doc/testing/longrun/p2p-longrun-soak-and-chaos.prd.md`、`doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md`。

<a id="s9a-gwsc-qa-receiver"></a>
### S9A：链上大世界状态底座自闭环
- 目标语义：本节的闭环对象不是单独的 libp2p/P2P transport，而是 `链上大世界状态底座`：P2P transport、分布式存储/blob closure、replication/gap sync/state sync、consensus/finality、execution record/receipt、observer/validator/storage ops，以及 API/viewer 对同一 world state 的投影。
- Claim boundary / 状态提交闭环首读入口：`doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md`。涉及 world state sync、commit closure、state-sync bundle、observer catch-up 或 API/viewer projection 的结论，先按 GWSC 口径定档，再下钻 S9/S10 执行套件。
- GWSC 的 [QA infrastructure receiver](doc/testing/prd.md#qa-infrastructure-receiver) 固定 source/candidate 与 evidence join；完整 17 QA cases、10 runtime groups、8 SC-9 cells 的目标映射见 [GWSC design](doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md)。设计/fixture/schema存在不等于实现或执行，required 与 full 环境差异按 [tier boundary](doc/testing/prd.md#qa-infrastructure-tier-boundaries) 判定。
- 可声明结论：
  - `module_required` 通过：底座本地合同可集成。
  - `module_full` 通过：底座在当前可执行 proxy/triad 拓扑下能持续推进和恢复。
  - `integration_required` 通过：真实大世界状态接到底座后，`action -> consensus -> execution -> receipt -> world state -> API/viewer` 没有明显漂移。
  - `release_full` 通过：真实环境具备公开测试候选所需的同窗口节点、manifest、readiness lane 与 claims-boundary 证据。
- 禁止声明：
  - S4/S9B required 绿，不得声明真实公网可达、physical NAT/CGNAT 已覆盖、`public_testnet ready` 或游戏整机体验成立。
  - S9/S9B proxy 绿，不得冒充 dedicated sentry/NAT lab、真实公网或 public testnet 证据。
  - 手工 copy validator `data/`、checkpoint 或 seed 只能作为 break-glass/recovery 证据，不得作为 live-candidate readiness。
- 开发/验证阶梯：
```text
Phase 0 contract inventory:
  world_id / chain_id / genesis / manifest
  action payload / consensus payload
  execution record / receipt / state hash
  blob/store closure
  peer head / gap sync / checkpoint
  validator / storage / observer role
  /v1/chain/status observability

Phase 1 single-node execution:
  action -> execution record -> receipt -> state hash
  replay same input -> same result
  rollback/checkpoint recovery

Phase 2 deterministic substrate contracts:
  oasis7_node / oasis7_net / oasis7_consensus / oasis7_distfs
  S9B required exact matrix

Phase 3 proxy multi-node substrate:
  S9 triad / triad_distributed soak
  chaos restart/pause/disconnect
  consensus hash, peer heads, gap sync, blob closure

Phase 4 state-sync / observer:
  high-head checkpoint or seed closure
  observer automatic catch-up
  storage blob availability and historical backfill

Phase 5 real-env ops:
  live local_peer_id, runtime sha256, service health
  connected peers, height/head progress, resource/traffic/wasm observability

Phase 6 world-state integration / release:
  S10 five-node real game soak
  public_testnet readiness lanes + same-window real-env evidence
```
- `module_required` 推荐命令：
```bash
./scripts/game-world-state-sync-commit-module-required.sh
```
- 等价展开命令：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --tests --features test_tier_required
env -u RUSTC_WRAPPER cargo test -p oasis7_node
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_net --features libp2p --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_consensus --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib
./scripts/p2p-mixed-topology-matrix.sh --tier required
```
- `module_full` 推荐命令：
```bash
./scripts/p2p-longrun-soak.sh --profile soak_smoke --topologies triad --duration-secs 600 --no-prewarm
./scripts/p2p-mixed-topology-matrix.sh --tier full
```
- state-sync / blob closure 推荐命令：
```bash
./scripts/p2p-verify-state-sync-closure.sh \
  --world-dir <seed-world-dir> \
  --execution-records-dir <seed-execution-records-dir> \
  --store-dir <seed-store-dir>
```
- `module_full` state-sync closure 证据包模板：`doc/testing/templates/state-sync-closure-evidence-packet-template.md`。复制模板到实际 evidence path 后回填；模板文件本身不能作为 pass evidence。
- real-env / release 入口：
```bash
./scripts/p2p-real-env-triad-snapshot.sh ...
./scripts/p2p-real-env-observability-monitor.sh ...
./scripts/s10-five-node-game-soak.sh --duration-secs 300 --no-prewarm --max-stall-secs 240 --max-lag-p95 50 --out-dir .tmp/release_gate_s10
./scripts/network-tier-public-testnet-readiness.sh --manifest <manifest> --lanes-tsv <lanes.tsv>
```
- S10 `summary.json` 固定输出 `api_viewer_projection`，S10 `summary.md` 固定输出 `API / Viewer Projection Contract`。默认状态是 `not_collected`，只有补齐同窗口 API/viewer evidence refs 后才能声明 projection 已验证。
- public_testnet readiness 必须包含 `api_viewer_projection_ready` lane；若缺失该 lane，或 pass evidence 仍指向 template/placeholder，或缺少 `api_viewer_projection.status=pass`、同窗口 refs 与 `world_state_projection_match=true` 的 JSON 证据，则不得声明 `ready_for_live_candidate`。
- 最小验收指标：
  - `committed_height` 单调推进；
  - `consensus_hash_consistent == true` 且 `consensus_hash_mismatch_count == 0`；
  - execution/state hash 与 receipt/event sequence 可追溯；
  - peer heads 非空且新鲜；
  - gap sync 成功且 replication error 不持续；
  - blob/store closure 完整；
  - observer 能自动追高；
  - checkpoint / rollback / state-sync 可恢复；
  - `/v1/chain/status` 能解释当前 readiness、degraded 或 blocker。
- 阻断签名：
  - `consensus_hash_divergence`；
  - `committed_height_not_monotonic`；
  - `known_peer_heads_zero_samples`；
  - `http_failure_samples`；
  - real-env 中 `sequencer_committed_height_zero`、`sequencer_execution_stale_height` 或同类 stale execution / stale peer-head；
  - readiness lane 为 `partial` / `block`；
  - manifest 仍是 example/template/placeholder/private-only endpoint；
  - 非同窗口 real-env 证据；
  - 通过手工复制数据目录获得的同步假象。

<a id="testing-p2p-mixed-topology-matrix"></a>

### S9B：P2P Mixed-Topology Matrix（P2PARCH-6）
- 当前状态（2026-04-07）：`scripts/p2p-mixed-topology-matrix.sh` 已把 `P2PARCH-6` 收口成一个可执行矩阵，区分 `exact` 与 `proxy` 两类覆盖，并把 shared-window / dedicated-lab / pass-uplift 外部证据与 blocker 语义写进 `summary.json`。
- 目标语义：
  - `exact`：直接运行当前仓库已经存在的 deterministic cargo tests，验证 private/validator_hidden/relay_only 边界、bootstrap poisoning、relay exhaustion 和 path failover。
  - `proxy`：运行当前可用的 triad/triad_distributed longrun 命令，给 mixed-topology recovery 留下可执行 full-tier drill；它不等价于 dedicated sentry/NAT lab。
- Path behavior taxonomy（2026-06-15 更新）：
  - `evidence_class`: `exact / proxy / manual_lab / real_env / unsupported`。`exact` 是 deterministic repo test；`proxy` 是当前可执行近似 longrun；`manual_lab` 需要专门 NAT/sentry lab；`real_env` 必须是同窗口真实网络证据；`unsupported` 表示该 topology 当前明确不承诺。
  - `path_expectation`: `must_direct / may_direct_must_recover / must_relay / must_not_publish_public_direct / manual_lab_required / unsupported`。该字段描述粗粒度 claim，不要求所有 NAT pair 都 direct-connect；case-specific route sequence 留在 `expected_route`。
  - `reachability_pair`: `public_direct / home_nat / cgnat / relay_only / validator_hidden / cloud_public / mixed_validator_observer`。
  - `degradation_class`: `none / sentry_loss / relay_exhaustion / degraded_latency_loss / restart_pause_disconnect / bootstrap_poisoning`。
  - `claim_boundary`: `required_exact / full_proxy / physical_nat_pending / shared_window_partial / public_testnet_blocked_or_ready`。
  - 这些字段属于 QA evidence taxonomy；canonical reachability/path label 必须来自 runtime peer reachability contract 或现有 `TransportPath` projection，matrix 脚本不得反向定义 runtime truth。
- 当前 full-tier 基线（2026-04-07 latest）：
  - latest full summary: `.tmp/p2p_mixed_topology_validation/20260407-120951-full/summary.json`
  - `required_exact_ready=true`
  - `full_proxy_ready=false`
  - latest failure signatures:
    - `sentry_loss_proxy_longrun`: `consensus_hash_divergence`, `committed_height_not_monotonic nodes=sequencer`, `known_peer_heads_zero_samples`, `http_failure_samples`
    - `mixed_topology_release_proxy`: `consensus_hash_divergence`, `committed_height_not_monotonic nodes=sequencer`, `known_peer_heads_zero_samples`, `http_failure_samples`
- 历史 real-env 基线（2026-04-08 incident chain terminal state；不是 current readiness authority）：
  - credentialed reconfirmation snapshot: `.tmp/p2p_real_env_triad/20260408-120134/summary.json`
  - historical provenance: `doc/testing/evidence/p2p-real-env-triad-incident-provenance-2026-07-31.md`
  - `claim_status=blocked`
  - latest real-env failure signatures:
    - `sequencer_committed_height_zero` / `sequencer_execution_stale_height` were the reconfirmation signatures; the absorbed chain's terminal residual was `fetch-commit` source readiness and peer/head reconvergence.
- 建议命令（required）：
```bash
./scripts/p2p-mixed-topology-matrix.sh --tier required
```
- 建议命令（full plan / 预览）：
```bash
./scripts/p2p-mixed-topology-matrix.sh \
  --tier full \
  --shared-window-evidence-ref doc/testing/evidence/public-testnet-current-required-lanes-2026-07-03.md \
  --shared-window-evidence-ref doc/testing/evidence/public-testnet-claims-boundary-review-2026-07-06.md \
  --dry-run
```
- 建议命令（full 执行）：
```bash
./scripts/p2p-mixed-topology-matrix.sh \
  --tier full \
  --shared-window-evidence-ref doc/testing/evidence/public-testnet-current-required-lanes-2026-07-03.md \
  --shared-window-evidence-ref doc/testing/evidence/public-testnet-claims-boundary-review-2026-07-06.md
```
- 建议命令（real env triad snapshot）：
```bash
P2PARCH6_SEQ_SSH_PASSWORD='***' \
P2PARCH6_STORAGE_SSH_PASSWORD='***' \
./scripts/p2p-real-env-triad-snapshot.sh \
  --samples 4 \
  --interval-secs 5 \
  --out-dir .tmp/p2p_real_env_triad
```
- 建议命令（real env triad complete observability）：
```bash
P2PARCH6_SEQ_SSH_PASSWORD='***' \
P2PARCH6_STORAGE_SSH_PASSWORD='***' \
./scripts/p2p-real-env-observability-monitor.sh \
  --samples 4 \
  --interval-secs 5 \
  --traffic-samples 3 \
  --traffic-interval-secs 20 \
  --window-minutes 10 \
  --out-dir .tmp/p2p_real_env_observability
```
- 脚本 smoke：
```bash
./scripts/p2p-mixed-topology-matrix-smoke.sh
```
- 通过标准：
  - live 执行时 `summary.json.overall_status == "ok"` 且 `totals.failed_count == 0`；
  - `required` 档位下所有 case 必须为 `coverage=exact`；
  - `full` 档位必须额外包含 `coverage=proxy` 的 longrun case，并在 evidence 中明确它们是 sentry-loss / mixed-topology live recovery 的当前代理，而不是 dedicated sentry/NAT lab 真值；
  - `summary.json.evidence_contract.executable_boundary.required_exact_ready` 必须为 `true`；
  - 若要宣称 full-tier proxy drill 已真实执行，则 `summary.json.evidence_contract.executable_boundary.full_proxy_ready` 必须为 `true`；
  - 若本轮 triad 仍是历史 mixed topology（本机 `observer` + 两台云端 validator），`summary.json.analysis.claim_mode` 应为 `observer_mixed_topology`，且若要把当前 `1` 本机 + `2` ECS real env 计入可审计 baseline，`summary.json.claim_status` 至少要达到 `partial_with_observer_blocker`，并明确写出真实 blocker；
  - 若本轮 triad 已切到三节点等权 validator，`summary.json.analysis.claim_mode` 应为 `three_equal_validator`，且若要计入该 baseline，`summary.json.claim_status` 应达到 `pass_candidate`；
  - 若要宣称本机 mixed-topology observer 接入已打通，则 real-env summary 里不得再出现 `observer_known_peer_heads_zero`、`observer_network_committed_height_zero`、`observer_committed_height_not_advancing`；
  - 若要宣称三节点等权 validator 拓扑已打通，则 real-env summary 里不得再出现 `local_committed_height_zero`、`local_known_peer_heads_zero`、`local_network_committed_height_zero`、`local_no_recent_progress_signal`、`triad_not_all_validator_roles`；
  - 若要继续给 network-rehearsal lane 提供 uplift 输入，`summary.json.external_evidence.shared_window_evidence_refs` 必须明确列出 same-window refs，且 `summary.json.evidence_contract.claim_readiness.shared_network_pass_blockers` 只能保留经审计接受的剩余 blocker；
  - 产物目录下必须同时有 `summary.json`、`summary.md`、`cases/<case_id>/command.txt`，live 执行还必须留下 `stdout.log/stderr.log`。
- 当前 exact case 入口：
  - `nat_private_role_policy`
  - `validator_hidden_boundary`
  - `relay_only_lane_budget`
  - `cgnat_relay_path_ranking`
  - `bootstrap_poisoning_dedupe`
  - `relay_budget_detection`
  - `path_failover_selection`
- 当前 proxy case 入口：
  - `sentry_loss_proxy_longrun`
  - `mixed_topology_release_proxy`
- 产物路径：
  - `.tmp/p2p_mixed_topology/<timestamp>-<tier>/summary.json`
  - `.tmp/p2p_mixed_topology/<timestamp>-<tier>/summary.md`
  - `.tmp/p2p_mixed_topology/<timestamp>-<tier>/cases/<case_id>/`
  - `.tmp/p2p_real_env_triad/<timestamp>/summary.json`
  - `.tmp/p2p_real_env_triad/<timestamp>/summary.md`
  - `.tmp/p2p_real_env_triad/<timestamp>/nodes/<label>/`
  - `.tmp/p2p_real_env_observability/<timestamp>/host/<timestamp>/summary.json`
  - `.tmp/p2p_real_env_observability/<timestamp>/traffic/latest_summary.json`
  - `.tmp/p2p_real_env_observability/<timestamp>/wasm/<label>/latest_summary.json`
  - `.tmp/p2p_real_env_observability/<timestamp>/report/latest_summary.json`
  - `.tmp/p2p_real_env_observability/latest_summary.json`
- summary 关键字段：
  - `external_evidence.shared_window_evidence_refs`
  - `external_evidence.dedicated_lab_evidence_refs`
  - `external_evidence.pass_uplift_decision_ref`
  - `evidence_contract.executable_boundary.required_exact_ready`
  - `evidence_contract.executable_boundary.full_proxy_ready`
  - `evidence_contract.executable_boundary.stronger_full_tier_truth_ready`
  - `evidence_contract.claim_readiness.mixed_topology_full_tier_status`
  - `evidence_contract.claim_readiness.stronger_full_tier_truth_blockers`
  - `evidence_contract.claim_readiness.shared_network_pass_blockers`
  - `claim_status`
  - `failure_signatures`
  - `cases[*].evidence_class`
  - `cases[*].path_expectation`
  - `cases[*].reachability_pair`
  - `cases[*].degradation_class`
  - `cases[*].claim_boundary`
  - `analysis.cloud_pair_service_healthy`
  - `analysis.cloud_pair_chain_visible`
  - `analysis.cloud_pair_progress_signal_present`
  - `analysis.claim_mode`
  - `analysis.local_peer_visibility_ok`
  - `analysis.local_network_commit_visible`
- complete observability summary 关键字段：
  - `overall.status`
  - `overall.alerts`
  - `overall.optimization_candidate_count`
  - `optimization_candidates[*].node_label/module/key/severity`
  - `nodes.<label>.host.runtime_cpu_percent`
  - `nodes.<label>.host.runtime_cpu_core_ratio`
  - `nodes.<label>.host.mem_available_percent`
  - `nodes.<label>.host.storage_used_percent`
  - `nodes.<label>.traffic.payload_total_bytes`
  - `nodes.<label>.traffic.control_plane_total_events`
  - `nodes.<label>.wasm.top_hotspot`
  - `nodes.<label>.modules.consensus.height_lag`
  - `nodes.<label>.modules.replication.recent_error_count`
  - `nodes.<label>.modules.p2p_reachability.selected_path_kind`
  - `nodes.<label>.modules.p2p_reachability.selected_path_age_ms`
  - `nodes.<label>.modules.p2p_reachability.path_transition_counters`
  - `nodes.<label>.modules.p2p_reachability.active_path_mix`
  - `nodes.<label>.modules.p2p_reachability.recent_fallback_reason`
  - `nodes.<label>.modules.p2p_reachability.reachability_confidence`
  - `nodes.<label>.modules.transactions.pending_count`
  - `nodes.<label>.modules.traffic_control_plane.control_plane_wire_ratio`
  - `nodes.<label>.optimization_candidates[*].key`
- 边界说明：
  - 当前仓库还没有 dedicated sentry role live harness，也没有物理 NAT/CGNAT 实验编排；因此 `proxy` 只代表“现在可执行的近似恢复 drill”，不能拿来冒充完整 mixed-topology 实证。
  - 2026-04-07 latest full run 已证明 matrix 能真实执行到 proxy soak，但当前 proxy drill 仍会因为 `consensus_hash_divergence / committed_height_not_monotonic / known_peer_heads_zero_samples / http_failure_samples` 失败；在这些签名被修平前，`P2PARCH-6` 仍不能宣称 `full_proxy_ready=true`。
  - 2026-04-08 historical real-env triad incident chain 已证明当时可重复采集 same-window 三节点样本，而且本机 observer 不再是主 blocker；`execution driver received stale height` 是链中间阶段签名，后续 rollout 后的终态 residual 是 `fetch-commit` source readiness、peer/head reconvergence 与 storage restart retry ordering。这组历史证据不能升级当前 lane 为 `pass`；任何当前判断都必须重新采集 same-window evidence。
  - complete observability monitor 只解决当前 triad 的 repo-owned 资源/状态/流量/WASM 统一监控，不等价于已经补齐 Prometheus/OTel/长期告警平台。
  - reachability/path observability 只能消费 `/v1/chain/status.observability` 的 bounded projection；字段缺失时应输出 `not_reported` 或等价状态，不允许 report helper 自行重建 peer path truth。
  - `relay_reserved`、`proxy` case pass、或 control-plane byte split 都不能单独支撑 `public reachable` / physical NAT / public_testnet readiness claim；这类 claim 必须回到 matrix taxonomy、same-window evidence 与 producer/QA pass-uplift decision。

### S9C：P2P 用户模式自动选择验证（P2PARCH-8/P2PARCH-9）
- 当前状态（2026-04-07）：
  - `P2PARCH-8` 已完成文档冻结：用户层默认只暴露 `自动加入 / 私有安全 / 公网入口` 三档简单模式，底层继续保留 `deployment_mode/node_role` 正式语义。
  - `P2PARCH-9` 已接通 viewer/launcher UX：`oasis7_web_launcher` 会把 chain runtime 的 P2P recommendation payload 一并透传给 `oasis7_client_launcher`，客户端会展示 requested/recommended/applied user mode、底层 role mapping 和 detection rationale，并为 `public_entry` 提供显式接受/拒绝路径。
- required 验证（本轮 docs-only baseline）：
```bash
rg -n "自动加入|私有安全|公网入口|deployment_mode|node_role|AutoNAT|高风险职责|显式确认" \
  doc/p2p/network/mainnet-private-reachability-architecture.prd.md \
  doc/p2p/network/mainnet-private-reachability-architecture.prd.md \
  testing-manual.md
./scripts/doc-governance-check.sh
git diff --check
```
- `P2PARCH-8` 通过标准：
  - PRD / project / testing manual 对用户可见模式、内部正式角色语义、默认全自动路径三者口径一致。
  - 文档明确要求普通用户默认路径不必理解 `deployment_mode/node_role`。
  - 文档明确要求涉及 `公网入口`、`relay`、`sentry` 等外部暴露职责时必须显式确认，不能静默自动升级。
- `P2PARCH-9` required 覆盖要求（实现落地时必须补 executable required evidence）：
  - 启动后 `60s` 内至少完成一次 reachability 检测，并在默认路径给出一个用户模式建议。
  - 当 AutoNAT、公网端口探测与打洞结果互相矛盾时，默认回退到保守模式，不能直接提升为 `公网入口`。
  - 当检测结果建议承担 `公网入口` 或等价暴露职责时，UI/CLI 必须要求显式确认，并记录检测依据与风险提示。
  - 用户拒绝 `公网入口` 后，系统必须回退到非入口模式，同时不破坏底层 `deployment_mode/node_role` 权限边界。
- `P2PARCH-9` full 覆盖要求（实现落地时必须补 executable full evidence）：
  - 至少覆盖 `公网可达`、`受限 NAT 可打洞`、`对称 NAT/CGNAT 无法打洞`、`检测结果抖动或冲突` 四类 reachability 场景。
  - 覆盖用户从默认推荐切到高级设置覆盖，再回退到自动模式的往返路径，并验证审计证据持续可读。
  - 覆盖多节点混合场景中，非公网节点、公网入口节点与 relay/sentry 语义的映射一致性，避免 UI 用户模式与底层 role policy 脱节。
  - 若复用 network-rehearsal / mixed-topology lane 作为 full-tier 证据，summary 中必须单独标注哪些 case 验证的是用户模式推荐，哪些 case 验证的是底层 reachability 真值。
- `P2PARCH-9` executable full-tier 基线（2026-04-07）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_web_launcher -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_client_launcher -- --nocapture
env -u RUSTC_WRAPPER cargo check -p oasis7_client_launcher --target wasm32-unknown-unknown
```
- 当前 full-tier 对账重点：
  - `oasis7_web_launcher` 必须把 `/v1/chain/status` 内的 P2P recommendation/evidence 正确代理到 `/api/state`
  - `oasis7_client_launcher` 必须正确渲染 requested/recommended/applied user mode、底层 `deployment_mode/node_role_claim` 与 rationale
  - `public_entry` 在 launcher 配置层必须要求显式确认；未确认时 `public_entry` 启动路径必须被拒绝
  - `auto_join -> recommended public_entry -> reject -> keep non-entry mode` 与 `auto_join -> accept public_entry -> restart apply` 这两条 UX 路径必须至少有单测或自动化闭环覆盖
- 证据要求：
  - docs-only 阶段至少留下 `rg` 命中结果与文档门禁通过记录。
  - 实现阶段至少留下模式推荐结果、触发确认的风险提示文本、用户接受/拒绝后的最终模式，以及对应的检测依据摘要。
  - 若当轮仍无法提供 dedicated NAT/public-entry lab，则必须在 evidence 中明确哪些结果来自 proxy/network-rehearsal drill，不能冒充真实公网探测实验。
- 反作弊/反女巫证据链门禁（TASK-GAME-015）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required runtime::tests::governance::governance_identity_penalty_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required governance_identity_penalty_and_appeal_drive_vote_rights -- --nocapture
```
- 通过标准：
  - 同目标主体 + 同证据哈希的惩罚重放被拒绝（incident 指纹不重复通过）。
  - 惩罚 -> 申诉 -> 复核后 `evidence_chain_hash` 逐阶段变化且 `appeal_evidence_hash/resolution_evidence_hash` 非空。
  - `governance_identity_penalty_monitor_stats` 输出误伤率与高风险未闭环数量。
- 经济源汇审计与阈值门禁（TASK-GAME-016）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required main_token_economy_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required main_token_treasury_distribution_applies_closed_loop_and_records_audit -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required main_token_fee_settlement_burns_supply_and_tracks_treasury_buckets -- --nocapture
```
- 通过标准：
  - 审计报表输出 `mint_total/burn_total/net_flow` 与当期 `issued/distributed` 指标。
  - `enforce_main_token_economy_gate` 在 `inflation:*` 或 `arbitrage:*` 告警时返回阻断错误。
  - 报表中 `exploit_signature` 可用于治理升级与 runbook 分诊。
- 可运维发布阻断门禁（TASK-GAME-017）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required longrun_operability_release_gate_ -- --nocapture
```
- 通过标准：
  - `evaluate_longrun_operability_release_gate` 产出统一报告，覆盖 `SLO + 告警 + 灾备演练 + 灰度阶段 + 经济告警`。
  - `enforce_longrun_operability_release_gate` 对首个违规项返回阻断错误（包含 `gate + reason`）。
  - 报告中的 `economy_report.alerts` 会同步升级为发布阻断违规项。

### S10：五节点真实游戏数据在线长跑技术套件
- 当前状态（2026-02-28）：`scripts/s10-five-node-game-soak.sh` 已恢复为可执行脚本，底座为五进程 `oasis7_chain_runtime`。
- DistFS probe 边界：reward worker 在空 blob set 时可幂等写入非敏感 seed，为 `distfs_total_checks` 提供采样前提；seed 成功不是 pass。当前窗口仍无样本时必须保留 `insufficient_data`，最终结论由完整 S10 metric gate 决定。
- 时间语义说明：S10 与 S9 口径一致，`slot_duration_ms/ticks_per_slot` 决定 PoS 逻辑时间，`node_tick_ms` 仅作轮询/回退间隔。
- 建议命令（smoke）：
```bash
./scripts/s10-five-node-game-soak.sh --duration-secs 600 --no-prewarm
```
- 建议命令（默认长窗）：
```bash
./scripts/s10-five-node-game-soak.sh
```
- 发布门禁基线命令（2026-02-28，300s）：
```bash
./scripts/s10-five-node-game-soak.sh --duration-secs 300 --no-prewarm --max-stall-secs 240 --max-lag-p95 50 --out-dir .tmp/release_gate_s10
```
- 通过标准：
  - 命令返回 `rc=0`；
  - `summary.json` 中 `run.status == "ok"`，并产出 `timeline.csv`；
  - `summary.json` 中 `run.metric_gate.status == "pass"`（一般告警通过 `run.metric_gate.notes` 留痕，不应降级为 `insufficient_data`）；
  - 若失败，必须保留 `failures.md` 作为分诊依据。
- metric-source 目标边界（当前脚本差距见 S9）：S10 的 mint、DistFS、settlement 与 invariant 只以
  `reward_runtime` 原生快照为准；每节点累计最大值聚合、缺失节点告警、
  `distfs_total_checks=0 -> insufficient_data`、以及 invariant/HTTP/运行状态的
  独立分类均与 S9 一致。历史 run 或空集 probe seed 不能替代本次完整 metric gate。
- 参考文档：`doc/testing/longrun/s10-five-node-real-game-soak.prd.md`、`doc/testing/longrun/p2p-longrun-soak-and-chaos.prd.md`、`doc/testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md`。

### 发布门禁一键收口（S0 + S1 + S6 + S9 + S10）
```bash
./scripts/release-gate.sh
./scripts/release-gate.sh --quick
./scripts/release-gate.sh --dry-run
```
- 默认串行执行：`ci-tests full`、`sync-m1/m4/m5 --check`、Web strict、S9/S10。
- `release-gate-web-strict.sh` 的两个 Web 阶段使用隔离的签名 WorldService QA 后端：先执行现有 `build:viewer:visual-test` 生成真实 WebGL2 bridge 与 `.viewer-test-dist`（不改写 tracked `viewer.js`），再显式编译 `test_tier_required` 的 ignored helper，启动真实 loopback HTTP handler，并将六项普通 `public` read 配置通过临时 `0700` 目录中的 `0600` 文件交给默认生产 launcher/Viewer。后端按真实 Node tick 推进；这验证 Hosted Web 消费者入口和 realtime UI，不代表正式世界部署、玩家所有权激活或 provider 自治验收。测试不传外部链状态 bind、世界目录或工程 scope，结束时删除私配置并有界停止自有进程组。
- `viewer-primary-web-entry-regression.sh` 专用于该隔离后端；`viewer-software-safe-step-regression.sh --release-web-fixture` 才启用同一路径，通用 `--url` / stack 参数仍使用调用者的原有运行源。primary/strict 的 `--scenario` 与 release 总入口的 `--web-scenario` 已移除；显式 release fixture 也会拒绝 step 的 debug scenario 参数；不得将它报告为指定 scenario 的验证。启动等待会立即报告 launcher/fixture 退出码和日志。
- `--quick` 只缩短 S9/S10 时长；它不向 `release-gate-web-strict.sh` 传递 visual/quick 开关，也不关闭 Web visual baseline。Web 阶段仍按选中的 release steps 与 strict 入口执行，不能从 quick 推断视觉检查被跳过。
- `--skip-*` 只用于已知外部约束或分片排障；必须在 summary 中保留 skip reason 与 claim boundary。跳过 `ci_full`、sync、Web strict、S9 或 S10 后，不得把本次结果写成完整 release coverage。

<a id="network-tiers-validation-evidence"></a>

### Network Tiers / Shared-Network Evidence
- 当前网络层真值统一以 `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md` 与对应 project/runbook 为准：
  - operator/runtime network-tier 是 `local_devnet -> public_testnet -> mainnet`，不作为玩家世界模型。
  - `public_testnet_rehearsal` 只作 legacy/rehearsal evidence，不能替代 formal `public_testnet` 的 11-lane readiness，也不代表 live `public_testnet`、`mainnet`、public launch、赛季上线或公开大世界已建立。
- Canonical docs:
  - Current network-tier source of truth: `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md`
  - `public_testnet` live-candidate checklist: `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.runbook.md`
  - Historical network-rehearsal records and archive boundary: `doc/testing/evidence/README.md`
  - Benchmark background: `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md`
- Canonical commands:
```bash
./scripts/network-tier-manifest.sh validate \
  --manifest doc/testing/templates/network-tier-public-testnet.example.json
./scripts/network-tier-public-testnet-readiness.sh \
  --manifest doc/testing/templates/network-tier-public-testnet.example.json
./scripts/network-tier-exit-review.sh \
  --manifest doc/testing/templates/network-tier-public-testnet.example.json
./scripts/network-tier-manifest-smoke.sh
```
- v2 planned 配置验证沿用 manifest smoke 及 Rust schema/CLI/launcher 回归，合同见[计划配置与运行支持边界](doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md#p2p-tier-planned-world-policy)。schema 合法不表示已激活：启动入口必须在写入前拒绝，readiness/exit-review 与旧证明入口不得给出可运行/已提交结论。实际单权威、独立故障域恢复及正式 profile verifier 仍需后续专项验收。
- Testnet-connected hosted entry flow:
  - 本机暴露 `hosted_public_join` / hosted-login 形态入口时，先证明本机节点已按 formal `public_testnet` manifest / `world_id` / `chain_id` / genesis / bootstrap peers 接入并同步到 testnet，再证明 hosted-login / launcher / viewer / pure API 的 runtime/status/API endpoint 指向该节点 world state。
  - 最小证据包括：manifest validation、local observer sync/preflight 输出、节点 health/status、connected peers、height/head 推进、hosted-login `login/start -> login/complete` smoke、以及 viewer / pure API 读取同一 testnet world state 的截图或 JSON 摘要。
  - 若缺少节点同步证据或 hosted-login 接入面仍指向 local execution world，只能记为 local hosted-public-join / hosted-login smoke，不得支撑 `public_testnet` unified-world 测试 claim。
- Boundary:
  - `testing-manual.md` is an execution index only; do not duplicate current network-tier verdicts here.
  - `public_testnet_rehearsal` history may explain historical technical benchmark context, but is not L5 evidence; current public readiness belongs to formal `public_testnet` readiness docs.

<a id="testing-decentralized-module-release-runtime-alerts"></a>

### S11：去中心化模块发布运行与告警（world-runtime）
- 适用范围：线上模块发布（`proposal -> attestation -> apply`）与 builtin 在线清单加载故障分诊。
- 生产执行边界（强制）：
  - 生产发布写入/激活只能由发布节点提交链上动作（`ModuleReleaseSubmit*` / `ModuleReleaseApply`）完成。
  - 主 CI 仅允许执行 `--check` 类回归与对账，不参与生产发布写入、阈值签名或激活判定。
- 节点侧固定验收入口（默认 required，按需追加 full）：
```bash
./scripts/module-release-node-acceptance.sh
./scripts/module-release-node-acceptance.sh --include-full
./scripts/module-release-node-attestation-flow.sh --help
./scripts/package-module-release-attestation-proof.sh --help
./scripts/submit-module-release-attestation.sh --help
./scripts/wasm-release-evidence-report.sh --expected-runners linux-x86_64
./scripts/wasm-release-evidence-report.sh \
  --skip-collect \
  --summary-import-dir output/ci/m1-wasm-summary \
  --module-sets m1 \
  --expected-runners linux-x86_64
./scripts/wasm-release-evidence-report.sh \
  --skip-collect \
  --summary-import-dir output/ci/m1-wasm-summary \
  --module-sets m1 \
  --expected-runners linux-x86_64,darwin-arm64
./scripts/module-release-node-attestation-flow.sh \
  --module-sets m1 \
  --summary-import-dir output/ci/m1-wasm-summary \
  --skip-local-collect \
  --required-runners linux-x86_64 \
  --expected-runners linux-x86_64,darwin-arm64 \
  --request-id 17 \
  --operator-agent-id operator-1 \
  --signer-node-id attestor-node-1 \
  --build-manifest-hash <hex> \
  --source-hash <hex> \
  --wasm-hash <hex> \
  --builder-image-digest <sha256:digest> \
  --container-platform linux-x86_64 \
  --canonicalizer-version strip-custom-sections-v1
```
- 产物与证据：
  - 默认输出目录：`.tmp/module_release_node_acceptance/<timestamp>/`
  - 最小归档：`summary.md`、`summary.json`、各 step log（含 triage 信号检索）
  - node-side attestation flow 默认输出目录：`.tmp/module_release_node_attestation_flow/<timestamp>/`
  - node-side attestation flow 最小归档：`flow_summary.md`、`flow_summary.json`、`staged_summaries/`、`proof_inputs/`、`proof/proof_payload.json`、`proof/submit_request.json`
  - attestation proof payload 默认输出目录：`.tmp/module_release_attestation_proof/<timestamp>/`
  - attestation proof 最小归档：`proof_payload.json`、`submit_request.json`、`evidence/` 附件目录或对应 archive、稳定 `proof_cid`
  - WASM release evidence 默认输出目录：`.tmp/wasm_release_evidence_report/<timestamp>/`
  - WASM release evidence 最小归档：`summary.md`、`summary.json`、`module_sets.tsv`、各 module set verify log 与 per-runner summary json
- 等价拆分命令（便于定向排障）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 module_release_submit_attestation_ --features test_tier_required -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 module_release_apply_rejects_when_attestation_threshold_not_met --features test_tier_required -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 module_release_apply_rejects_when_attestation_receipt_evidence_mismatches --features test_tier_required -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 power_bootstrap_release_manifest_full --features test_tier_full -- --nocapture
./scripts/module-release-node-attestation-flow.sh --help
./scripts/package-module-release-attestation-proof.sh --help
./scripts/submit-module-release-attestation.sh --help
./scripts/ci-m1-wasm-summary.sh --module-set m1 --runner-label linux-x86_64 --out <summary-dir>/m1/linux-x86_64.json
python3 ./scripts/ci-verify-m1-wasm-summaries.py --module-set m1 --summary-dir <summary-dir>/m1 --expected-runners linux-x86_64
```
- finality 基准固定入口（`stake/epoch` 验签耗时 + 2 epoch 收敛）：
```bash
./scripts/oasis7-runtime-finality-baseline.sh
./scripts/oasis7-runtime-finality-baseline.sh --required-samples 1 --full-samples 1 --warmup-samples 0
```
- 基准产物：
  - 默认输出目录：`.tmp/world_runtime_finality_baseline/<timestamp>/`
  - 归档文件：`summary.md`（人读）与 `summary.json`（机器读）
- 运行时分诊检索（日志/审计）：
```bash
rg -n "conflicting attestation already exists|attestation threshold not met|attestation receipt evidence mismatch|fault_signature=builtin_release_manifest_" output .tmp
```
- 告警策略（发布阻断）：
| 场景 | 识别信号（日志/事件） | 阻断策略 | 首轮处置 |
| --- | --- | --- | --- |
| 证明冲突 | `module release attestation rejected: conflicting attestation already exists for signer=<id> platform=<platform>` | 阻断对应 `request_id` 的继续激活 | 冻结该 `request_id`，核对 `build_manifest_hash/source_hash/wasm_hash/proof_cid`，保留首条证据并重新发起发布单。 |
| 阈值不足 | `module release apply rejected: attestation threshold not met epoch_id=<id> threshold=<n> aggregated_signers=<m>` | 阻断 `ModuleReleaseApply`，保持旧 `active_manifest_hash` | 对齐当前 `epoch` 快照 signer 集，补齐缺失 signer 证明后重试 apply。 |
| 证明未入链 | 只有 CI / workflow artifact，缺少 node-side `proof_payload.json` 或未执行 attestation submit | 不得进入 `ModuleReleaseApply` | 先用 proof 脚本打包正式证据，生成稳定 `proof_cid`，再由发布节点提交 `ModuleReleaseSubmitAttestation`。 |
| manifest 不可达/回滚/漂移 | `fault_signature=builtin_release_manifest_unreachable` / `fault_signature=builtin_release_manifest_missing_or_rolled_back` / `fault_signature=builtin_release_manifest_identity_drift` | 阻断 builtin 新版本加载，维持旧版本 | 检查 distfs artifact 可达性、release manifest 条目与 identity 是否一致，修复后再触发加载。 |

## Human/AI 共用执行剧本

### 阶段 A：确定测试范围
1. 按前部“默认测试选择树”识别实际影响与消费者。
2. 生成本次适用的套件和定向回归列表。
3. 在日志中写清“为什么跑这些、不跑哪些”。

### 阶段 B：先跑低层，后跑高层
1. 先执行 S0。
2. 再执行对应的 L1/L2/L3 套件（S1/S2/S3/S4/S5）。
3. 最后执行 UI 闭环与压力（S6/S8；分布式改动需补 S9 或 S10）。
4. 任意层失败立即停止上层，先定位并修复。

### 阶段 C：记录结论
1. 对每个套件记录：命令、结果、失败点、是否复跑。
2. 记录证据路径（截图、console、CSV、关键日志）。
3. 给出“是否达到本次任务充分度标准”的结论。

## 证据规范

### 必备证据
- 命令执行记录（终端或 CI 日志）。
- 失败堆栈或关键断言信息。
- UI 闭环截图与 console 结果（若执行 S6）。

### 推荐证据目录
- `output/playwright/viewer/*.png`
- `output/playwright/viewer/console.log`（或等价重定向日志）
- `.tmp/viewer_owr4_stress/<timestamp>/`
- `.tmp/llm_stress/`
- `.tmp/p2p_longrun/<timestamp>/`
- `.tmp/s10_game_longrun/<timestamp>/`

### 结果记录模板
```md
- 目标变更：
- 触发路径：
- 执行者（Human/AI）：
- 套件清单（S0~S10）：
  - Sx: 命令 / 结果 / 证据路径
- 失败分诊：
  - 层级（L0~L5）：
  - 原因分类（确定性/环境/flaky）：
  - 处理结论：
- 最终结论：
- 遗留事项：
```

## 失败分诊（按层）
1. L0 失败：优先修复格式、工件、目标安装问题。
2. L1 失败：优先定位业务逻辑回归或断言漂移。
3. L2 失败：优先检查协议兼容、连接时序、桥接参数。
4. L3 失败：优先检查分布式状态恢复、签名校验、网络行为。
5. L4 失败：先判定是否环境问题（端口、launcher 进程、wasm 初始化），再判定 UI 回归。
6. L5 失败：只针对真实人类或受控外部玩家样本，先核验样本、环境、继续游玩意愿与反馈采集；性能、资源泄漏和长时状态累计问题回到对应技术套件（S8/S9/S10）分诊。

## TODO（待收口）
- [x] TODO-1：修正 S7 场景矩阵回归命令的覆盖口径。
  - 处理结果（2026-03-05）：S7 的 `oasis7_init_demo_runs_` 已切换到 `test_tier_full` 执行档位。
  - 验收记录：`env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_full oasis7_init_demo_runs_ -- --nocapture` 命中多场景用例（非 1 条）。

- [x] TODO-2：修复 S5 Pixel World Bridge（Bevy）测试编译阻塞。
  - 处理结果（2026-02-21）：当前 `pixel_world_bridge` lib 测试已恢复可编译可执行，并纳入 `scripts/ci-tests.sh` 的 full support-crate shard。
  - 验收记录：`env -u RUSTC_WRAPPER cargo test -p pixel_world_bridge --lib` 通过。

## 风险
- 风险 1：把 `required/full` 当作整应用全覆盖。
  - 缓解：按默认测试选择树覆盖实际影响面，明确未执行部分。
- 风险 2：UI 闭环只看截图，不看状态与 console。
  - 缓解：S6 强制 `console error = 0` + 可见状态判断。
- 风险 3：分布式子系统改动未触发对应 crate 测试。
  - 缓解：按前部默认测试选择树验证受影响子系统和消费者。
- 风险 4：压力回归长期缺失，问题只在长跑暴露。
  - 缓解：性能、资源压力、恢复和长窗行为按实际影响选择 S8/S9/S10；正式发布执行完整适用验收。

## 里程碑
- T1：完成基于仓库现状的分层模型与套件目录。
- T2：统一影响选择树与 Human/AI 共用剧本。
- T3：完善套件命令、结果含义和失败分诊规则。
- T4：后续按真实缺陷复盘持续调整各层用例配额与命令清单。

## 本地 CI 工具环境

普通 `required` 本地入口使用 Python 3.11 或更高版本（标准库 `tomllib` 解析 TOML）。确认 `python3 --version` 指向该版本；Rust、Node 与平台依赖按对应测试组准备。


<a id="planned-node-economic-contract-qa"></a>
## 节点既有经济合同 QA 场景（planned，未执行）

本节从既有节点积分、PowerCredit、创世分配/奖励专业接受条件抽取可逐项执行的 QA 方法，仅供后续获得适用授权的验证任务使用；没有新增经济功能、资格、参数、额度、审批、权限或当前能力结论。当前这些场景均未执行，文档检查不能使其通过。每项只能使用固定同一候选、配置版本、受控fixture及原专业合同指定的角色/阶段；不得在fixture中填入真实地址/密钥、绑定真实账户、启动实际铸造/兑换/奖励发放。真实经济操作始终另需原逐次授权及全部原前置，本手册不授予该授权。

业务oracle以各项链接的专业原文为准；本节不成为经济参数第二真值。正式验证者须保存候选commit、环境、fixture/输入和配置引用、适用验收与层级、预期/实际结果、拒绝原因和未覆盖范围。局部测试定义只能证明其列出的断言，不代表完整矩阵已通过。生产控制或审批证据缺失时如实保留pending/conditional/block，不能用fixture补真值。本任务不运行任何业务场景。

<a id="qa-economic-points-eligibility"></a>
### 基础义务与合格额外贡献分离：planned QA

Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；[准确专业接受关系](doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得SC-1..3、贡献输入与计分公式的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原 SC-1..3 与贡献输入分别构造自身模拟、代跑、维护、有效/无效存储和在线挑战/回退样例，逐项核对资格与计分；不得把自身义务或不合格贡献计成额外收益。 当前只定义方法，不执行该步骤。
- 原权威oracle：为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不能推出所有玩家获益、实际采样可信或公开网络安全；不得凭其他项green代签。
- 证据字段与层级：原贡献画像、输入、资格与得分对账；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-points-allocation"></a>
### 固定双池的确定性且不重复分配：planned QA

Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；[准确专业接受关系](doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得AC-9、独立存储奖励池与结算合并规则的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按 AC-8/9 的至少3节点、连续2epoch画像，分别覆盖存储池启用/关闭、严格阈值、最小挑战数、可选质押封顶、零合格贡献及惩罚；核对各池预算不超发、排序与累计单调，不假定余额自动结转。 当前只定义方法，不执行该步骤。
- 原权威oracle：两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不调整预算/权重/阈值，不证明实际数值平衡；不得凭其他项green代签。
- 证据字段与层级：逐epoch/node得分、两池分配及预算差额；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-points-recovery"></a>
### epoch结算与恢复不重复入账：planned QA

Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；[准确专业接受关系](doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得AC-7、Runtime 与多节点闭环约束的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原 runtime闭环约束对同一epoch/node注入重复tick、重启恢复、replay及不可读reward-runtime-state.json，对账awarded/cumulative不重复增加、快照不改ledger与明确warning/metric；结合3节点2epoch场景。 当前只定义方法，不执行该步骤。
- 原权威oracle：同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody；不得凭其他项green代签。
- 证据字段与层级：恢复前后ledger、settlement及降级记录；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-points-asset-gates"></a>
### 积分与独立资产路径保持门禁：planned QA

Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；[准确专业接受关系](doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得积分与资产结算边界的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原积分与资产边界分别核对PowerCredit身份/签名/预算/reserve/nonce/replay门禁和Token同epoch node_service_reward预算分配；双路径并用列出重复奖励审查，缺该审查不形成经济安全通过。 当前只定义方法，不执行该步骤。
- 原权威oracle：积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness；不得凭其他项green代签。
- 证据字段与层级：每条路径预算/授权/拒绝与独立审查记录；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-ledger"></a>
### 贡献形成受限协议资产账本：planned QA

Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；[准确专业接受关系](doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得AC-1..6、资产与储备账本和结算记录的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原test_tier_required铸造守恒场景对照awarded_points、points_per_credit、floor结果与mint record及余额，核对未获授权的资产路径不能形成有效铸造。 当前只定义方法，不执行该步骤。
- 原权威oracle：PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不更改兑换率，不新增转账/市场或当前可兑现承诺；不得凭其他项green代签。
- 证据字段与层级：积分/credits换算与mint/balance/record对账；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-redemption"></a>
### 全部条件成立才燃烧资产并增加目标电力：planned QA

Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；[准确专业接受关系](doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得兑换动作与事件、关键规则的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原required兑换扣账/电力一致、reserve不足、nonce重放拒绝场景，同时覆盖最小量和epoch上限；按full多节点多epoch压力对账原动作及事件结果。 当前只定义方法，不执行该步骤。
- 原权威oracle：兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不改额度、目标、权限或承诺当前服务可用；不得凭其他项green代签。
- 证据字段与层级：credits/reserve/electricity与拒绝原因；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-signature"></a>
### 签名治理条件不足必须拒绝：planned QA

Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；[准确专业接受关系](doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得审计与签名治理演进、MIG-097验收的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原MIG-097的required/full验证v2/redeem签名、治理拒绝与快照策略，分别对缺签名、错误绑定、策略要求无法满足和历史v1兼容边界核验；不改变worker安全默认。 当前只定义方法，不执行该步骤。
- 原权威oracle：v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不接触密钥，不证明HSM/KMS或跨组织custody；不得凭其他项green代签。
- 证据字段与层级：版本化载荷、绑定/策略与拒绝结果；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-audit"></a>
### 审计与恢复检测异常而不自动修复：planned QA

Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；[准确专业接受关系](doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得状态与回放一致性、审计与签名治理演进、MIG-096验收的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原MIG-096 required/full审计、required快照余额一致及full重启replay/libp2p复制抽样，对比余额储备和记录，核对报告列异常且不自动修复/对账；恢复后重验策略。 当前只定义方法，不执行该步骤。
- 原权威oracle：mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不证明mainnet或生产恢复，不恢复历史viewer flags入口；不得凭其他项green代签。
- 证据字段与层级：快照前后状态、invariant report与策略复核；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-genesis-allocation"></a>
### 冻结分配可核算到各bucket与recipient：planned QA

Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；[准确专业接受关系](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得SC-1/8、AC-1/8/9/12/13、DEC-TOKEN-INIT-009的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原TOKEN-INIT-001/002与QA TGA-01/02/08，逐条核对七桶参数、比例合计、冻结绝对金额、rounding及recipient聚合，不使用手工补差或把custody写成treasury。 当前只定义方法，不执行该步骤。
- 原权威oracle：按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不变更总量/比例/账户，不执行InitializeMainTokenGenesis；不得凭其他项green代签。
- 证据字段与层级：固定参数版本、七桶金额与rounding对账；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。

<a id="qa-economic-genesis-control"></a>
### 控制主体与个人受益及释放边界分别可审计：planned QA

Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；[准确专业接受关系](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得SC-2..4、AC-2..5、DEC-TOKEN-INIT-002/004的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按QA TGA-03..07和原TOKEN-INIT-001/002核对控制主体、个人受益拆分、vesting/claim与创世/首年释放；缺主体、个人拆分或计划数值不得以比例表正确代签。 当前只定义方法，不执行该步骤。
- 原权威oracle：项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不改控制人/vesting/释放额度，不把协议储备当团队库存；不得凭其他项green代签。
- 证据字段与层级：真实控制映射、受益拆分、释放计划与阻断依据；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。

<a id="qa-economic-genesis-freeze"></a>
### 逻辑冻结不等于真实mint-ready：planned QA

Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；[准确专业接受关系](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得SC-6、AC-11、DEC-TOKEN-INIT-006、正式执行清单的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：planned依formal freeze sheet §5/§7、QA checklist §7的真实地址/controller/个人拆分/rounding/最终verdict逐项核对；任一TBD或conditional/block必须阻断，模板与历史勾选不代签。 当前只定义方法，不执行该步骤。
- 原权威oracle：slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易；不得凭其他项green代签。
- 证据字段与层级：绑定证据、完整pre-mint清单与该次QA verdict；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。

<a id="qa-economic-genesis-reward"></a>
### 独立reserve只消费最终复核的批准记录：planned QA

Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；[准确专业接受关系](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得AC-6/10、DEC-TOKEN-INIT-003/005、Execution Path Decision的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原TOKEN-INIT-003及canonical reward pack/ledger复核贡献证据、Approval ID、approved状态、distribution前actual-value review和最终Actual Amount；缺任何条件不得发放，执行后的Distribution Ref/date按原ledger闭合。 当前只定义方法，不执行该步骤。
- 原权威oracle：独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次；不得凭其他项green代签。
- 证据字段与层级：canonical批准行、最终金额复核、审批及distribution引用；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。


<a id="unified-world-resource-verification"></a>
## 统一 seed/resource 已采用子范围与正式目标验证计划

来源为 [unified 专业设计](doc/world-simulator/scenario/unified-world-seed-fragment-runtime.design.md#unified-verification-plan)。本节仅登记现有合同的计划与阻塞，不改测试层级、原验收或正式设计选择；本次未运行。所有命令先核固定候选的 test discovery，0 tests 不视为通过。

<a id="uws-v-local-spawn"></a>
### UWS-V-LOCAL-SPAWN：本地默认初始化

对固定 candidate 执行 `cargo test -p oasis7 --lib bootstrap_formal_release_runtime_world_uses_seeded_fragment_bootstrap_agent`，先确认 discovery 命中该具名测试。沿 [support.rs](crates/oasis7/src/viewer/runtime_live/support.rs) 原 fixture 核本地默认 Agent 坐标与 [gameplay_actions.rs](crates/oasis7/src/viewer/gameplay_actions.rs) 所取 seed-model 坐标，保存版本、有效配置、模型坐标、命令和原输出。该断言不验证正式 Agent location_id、链 provenance、claim 完整流程或玩家可读性；对应范围继续待验证，本次未运行。

该具名旧 fixture 仅是正例。负例计划尚待实现：在隔离测试构造中使运行时 Agent 坐标不同于同一 seed-model Agent 坐标，或保持 seed 不变而改有效场景配置后仍错误复用旧坐标；按本地接线原 oracle，该比较必须报告不一致而不能通过。负例用于验证比较/接线，不新增生产 fallback 或正式 claim 模型。无 frag/边界降级沿出生专题原验证，不由本 fixture 代签。保存受控差异、两组坐标和预期/实际 verdict；目前没有声称该负例测试已存在。

<a id="uws-v-artifact"></a>
### 11.2 UWS-V-ARTIFACT：采用分离的离线核验计划

由 artifact / QA owner 在待验候选中使用既有已批准测试 genesis 与尚不存在的临时输出路径（脚本拒绝已存在目录，不默认启用覆盖）。运行脚本 `create --genesis <fixture> --out-dir <temp> --world-scenario <existing-scenario> --merged-manifest-out <temp-manifest>`，再运行同脚本 `validate --world-dir <temp>/world --merged-public-manifest <temp-manifest>`；create 输出的 governed world 位于 `<temp>/world`，sidecar 位于同级 `<temp>/generated-scenario-world`，provenance 位于 `<temp>/world-generation-provenance.json`；validate 接收 world 子目录并从其父目录找 sidecar。具体参数沿脚本 usage，不填造 fixture、seed 或配置。保存 genesis 与 manifest hash、scenario/config/seed provenance、两目录 listing、audit 输出与退出码。断言 governed world 仍由 registry import/audit 路径拥有，generated-scenario-world sidecar 与 provenance 保留；不能以后一目录覆盖前者。缺 approved fixture 或脚本实际依赖时先记录 blocked，本计划不授权连接真实网络或激活世界。本次未运行。

负例计划只在复制的离线输出中单项改变：删除必需 snapshot/journal、将 artifact_kind 改为不符、将 seed 变成非正数、清空 config、改 public_manifest_sha256 或 entry_count。按 [现有 validate 源](scripts/p2p-build-governed-bootstrap-world.sh) 对应校验，必须非零拒绝并保存精确报错，未运行不记为已覆盖。另将 sidecar 目录误作 governed world 输入，或把 sidecar snapshot/journal 覆盖到复制的 governed 目录：核 registry audit 是否拒绝、两来源身份是否失配，分离 oracle 是绝不能把 sidecar 证据当 audited governed 身份。该替换负例尚无已核实测试；若工具未拒绝，也必须作为分离验证失败/实现缺口记录，而非把它解释成已统一或自动选择新承载。保存原/篡改目录 hash、provenance、audit/validate 命令与输出；不得改真实工件。

<a id="uws-v-status"></a>
### 11.3 UWS-V-STATUS：当前 status 来源正负例

源码与 [Cargo bin 配置](crates/oasis7_chain_runtime/Cargo.toml)对应：对固定候选使用 `cargo test -p oasis7_chain_runtime --features node-libp2p --bin oasis7_chain_runtime <filter>`，filter 分别为 `status_prefers_latest_execution_record_cas_snapshot_over_stale_execution_world_json`、`status_rejects_tampered_authoritative_execution_record_cas_snapshot`、`status_rejects_invalid_latest_execution_record_over_ready_legacy_json`、`status_rejects_newer_execution_record_with_stale_verified_snapshot_height`；原测试使用临时离线材料。先核 discovery 每个命中具名测试，再保存原输出及使用的 record/CAS/height 关系，0 tests、编译失败或环境阻塞不报通过。另用现有 `status_uses_runtime_execution_snapshot_when_simulator_mirror_is_empty` 检查其真实 fallback fixture，不把一个 fallback 覆盖全部 recovery。范围只为来源读取和拒绝旧 ready，不启动链或证明共识。本次未运行。

<a id="uws-v-consumers"></a>
### 11.4 UWS-V-CONSUMERS：正式目标的对账与阻塞记录

这是 R1–R6 的待实施验证计划，尚未核实完整可执行 harness，不伪装成已有测试。QA 分别记录 local/public/runtime-live 的候选、声明 profile、世界/链/genesis、effective seed/config/schema/算法版本、starter 选择及其批准来源。缺正式选择时对应场景标 blocked，不能以 Simulator 或本地默认替代。固定同 Agent、snapshot height/hash 与观察权限后取得 Viewer 原始 location/resource/seed/version 和 provider observation；按 R5 逐字段比较 IDs、附近资源与来源。兼容锚点、pending、provisional 分别核状态和正式动作拒绝，不自动等同 committed。

R3/R4 用正常、有明确 empty-world 状态、缺 manifest、未 committed 与资源/出生缺失场景逐步核 §6 等待/错误，不把注册成功当资源绑定。R6 取得真实生成 result/commit、一次消费 delta、刷新及 replay 前后 remaining，证明同一记录引用和余额未被 generation 复原；没有获准的正式 record 时停止该场景并记录具体缺口，不新增 record/schema。执行范围与权限由实际实现/QA 任务另行确认；本次只建立计划与来源，不执行业务操作。


<a id="uws-v-seed-reference"></a>
### UWS-V-SEED-REFERENCE：R1/AC1/AC5 的来源一致性

输入为同候选同 profile 的 Simulator、runtime snapshot 与 testnet replay/chain 材料，附已批准的 world/chain/genesis/manifest 关系、effective seed/config/schema/算法版本与 chunk coord/触发顺序。方法逐一追原引用再对照实际生成摘要；oracle 是同一有效输入产生一致 chunk/resource summary，且各入口引用同一 canonical seed 或其可验证派生证明，不能只比较最终数量。负例在隔离对照材料中分别换 world_id/genesis、seed、config hash 或 schema/version 中一项，或提供缺失的 seed 引用；必须记录引用/条件不一致，不能给 AC1/AC5 或 testnet readiness 的一致性通过。这里不定义新运行时错误码或替正式 seed 位置作决定。保存三份原材料、链上引用、每字段对比和缺失/不一致 verdict；正式承载未选或材料不足即 blocked，不能用本地 seed 常量填空。

<a id="uws-v-resource-budget"></a>
### UWS-V-RESOURCE-BUDGET：R2 的账本来源与不临时重算

输入为固定候选和同一有效 seed/config/order 的 Simulator model，选真实存在且有 fragment 与所属 chunk total/remaining 的地点，再取同 profile snapshot/consumer 可采量。方法核 fragment profile、fragment/chunk 预算以及 consumer 所用来源；对一个原支持且合法的消费场景分别保存动作前、权威结果后与刷新后账本。oracle 来自原 R2 和 chunked 库存合同：total 为原定量，remaining 取当前账本；同一次实际资源消费的 fragment 与所属 chunk 扣减对应，consumer 可采量不等于重新生成的初始 total。不得将 runtime_agent_chunk_v1 的合成预算冒充 Simulator 自然资源。

负例为非零已消耗后错误把初始生成预算作为当前可采量、缺 fragment/chunk budget 却临时重算、或把另一 chunk 的预算引用到当前 fragment。比较必须报告来源/余额不符，不能通过 R2；对正式可消费状态的处置继续由 R6/原动作合同决定，不在此新增 schema 或失败策略。保存原模型/来源引用、前后两级账本、真实动作结果、刷新 consumer 输出、反例差异与 verdict。实际消费/harness 尚未核实，只有 snapshot 具名 fixture 不足以覆盖上述矩阵，本次未运行。

<a id="uws-v-observation-parity"></a>
### UWS-V-OBSERVATION-PARITY：R5/AC4 同快照逐字段核对

输入固定 candidate/profile、同一 Agent、同一 snapshot height/hash 及同等合法观察权限；分别保存 Viewer 实际 model.locations/terrain/resource summary 和 provider observation。oracle 是同一 location/resource IDs、附近资源摘要与 world seed/version，并保留 compat/provisional 来源标记；隐藏 frag 低层细节不允许替换正式资源身份。方法在同一快照静止窗口采两输出，逐字段及引用对账；异步到达但版本不同先归为不可比，不强行判一致。

负例在隔离 harness 中使一方读取旧 snapshot、不同 location/resource ID、不同 seed/version 或将 compat anchor 标为正式资源来源，每次只改一项。比较必须显式报告对应 mismatch/stale/不可比，不能给 AC4 通过；正式动作是否拒绝另由 R6 和原权限/动作合同核验，不在本计划发明新 API 行为。保存两份完整原输出、snapshot/Agent/权限标识、受控差异、字段对照和 verdict。缺真实两消费者接线则 blocked，不能用同一 DTO 自比或两份模拟输出代签。
