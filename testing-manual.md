# Workflow Reference (Single Source)
- 本手册仅定义测试分层、套件入口和操作细节。
- 流程阶段、责任边界、必需/可选 gate、失败回退路径统一以 `doc/engineering/workflow/source-of-truth.md` 为准。
- 如测试流程规则与 source-of-truth 冲突，以 source-of-truth 为准，并先更新 source-of-truth。

# oasis7: 系统性应用测试手册（Human/AI 通用）

## 目标
- 基于仓库当前实现，提供可直接执行的分层测试手册，让人类开发者与 AI Agent 都能对“整应用”做充分测试。
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

Wait Admit 与 Resume 的崩溃恢复同样等待原请求的完整提交和精确 resumed memory，真实错误仍提前失败。初次 exit73 与恢复子进程分别使用独立 phase 日志和最多 60 秒的父进程硬截止；存在外层 completion deadline 时，两阶段继承同一绝对截止并预留清理时间，不刷新预算。没有外层截止时，每阶段各自有界，不将两阶段描述为合计 60 秒。持续观察使用 `Session::drain_for` 实际读取新 TCP 帧，保留原 exchange 的 Hello/Snapshot/Ack 顺序和未完整帧，不额外请求快照；已完成的 exchange 不能代替后续读取或重复计入新快照数量。

`real_tcp_hosted_wait_admit_crash_delayed_success_preserves_completion_oracle` 保留成功 View 的 1000 毫秒响应延迟，并在恢复观察开始后，以原 request digest 与 lease 绑定一个 13 秒的真实 Wait 时钟屏障。只有同一 canonical Admit 和 Settled lease 已确认时才等待；放行前不得存在已选 wake 或 resumed model。等待期间释放 driver 锁，不阻塞 RPC；随后沿原空 tick、wake、Resume、Act 和 memory 路径完成。该专用探针沿用外层 child 截止，普通 Wait 时钟仍为原 20 秒。恢复耗时在完成循环退出时冻结，排除清理耗时；旧 12 秒观察器必须在原完成判据上失败，不能靠调整阈值、改响应字节或延长 transport timeout 通过。

Wait capture crash 恢复保留原 12 秒观察截止及精确 memory、身份和单次效果断言。失败时 `hosted_wait_capture_observation_failure` 仅输出最后 16 个截止前样本：相对毫秒、白名单 phase/operation/I/O 状态、无符号计数、逐项完成条件布尔值（原短路未求值的 memory 为 null），以及清理后的 reader/drain/worker 状态；同时报告丢弃样本数，不能将尾部样本当完整阶段历史。清理后的成功不能改变截止前判定。未知枚举记为 unknown，非数字计数记为 null；不输出 memory、模型内容、请求身份、错误正文或 checkpoint。私有完整现场仍按原私有证据合同保管，不能直接作为公开 artifact 上传。

final-budget 夹具只为已结算的原 continuation 推进 canonical 空时钟；生成 wake 后，在同一次 driver 持锁期间记录该 wake 与原请求的选择关系，避免 App 已消费 wake 后夹具继续为旧等待推进时钟。`real_tcp_hosted_final_budget_clock_rescan_preserves_resumed_reserve` 通过子进程隔离的 rendezvous，强制 App 在时钟下一轮轮询前消费原 wake 并准备第二次 Reserve；它只控制线程交错，不改写请求或提交额外世界操作。通过仍须满足原 final-budget 写失败注入、18 项状态与 5 项 Runtime ledger 回滚、原身份恢复和清理断言，不以同步 marker 代替这些结果。

Wait rejection 的初次与恢复子进程同样使用既有 60 秒上限和嵌套清理预留，分别保存 `completion-wait-rejection-initial.*.log` 与 `completion-wait-rejection-recovery.*.log`，不提高 ordinary/runtime-tests 总时限。fixture 生命周期回调先复制 root 并释放锁，再进入 gate 状态；补偿 selector 不持 gate 锁读取 root。`http_fixture::lock_order_tests` 用 channel 固定补偿锁与真实 finished/joined 回调的交错，旧锁作用域应明确失败而非挂起。`application_completion::launcher_tests` 覆盖真实初次挂起、exit73 后恢复挂起与独立日志；原 canonical、原身份、写失败及 exit73 断言仍须通过。

清理采用独立 10 秒 watchdog，必须在 worker join、node stop 完成后显式取消；超时、提前退出或无法证明全部 HTTP worker 已停止时以非零状态终止测试进程，不能丢弃活线程后释放场景 permit。`cleanup_deadline::tests` 用独立子进程注入卡死 join、锁等待、未完成清理及满 stderr 下的退出，外层仍受既有 child 监督。清理诊断是 best-effort：100 毫秒后备退出不能等待日志，输出可能为空或截断；stage/reason/exit code 的完整格式由可控 writer 回归验证，不把日志存在误作有界终止的必要条件。stream Session 的 EOF drain 使用总计 2 秒截止，不以每次读取重新计时；保留 EOF 前排队的 1 MiB 响应回归，并用持续可读 peer 验证超时后仍完成 join。这些是失败收口边界，不替代业务通过条件；宿主调度或系统调用异常仍由外层进程监督兜底。

<a id="local-check-entry"></a>
## 显式本地检查：统一入口与验收范围

普通提交的 `./scripts/pre-commit.sh` 是兼容 no-op，直接成功退出不代表执行过检查。`./scripts/ci-tests.sh` 必须提供显式 tier，省略会报错；`commit` 是一次主动诊断，包含治理、consensus/distfs 与 Viewer contract/UI 等检查，不是自动提交门禁，也不包含 Rust fmt/完整 required。help 与本节均将其标为显式诊断。

| 当前目的 | 当前可执行入口 | 结果证明范围 |
| --- | --- | --- |
| 文档改动反馈 | `./scripts/doc-governance-check.sh` | 实际检查的结构、引用和治理规则；不证明文档语义或功能实现 |
| 最小 Rust 包反馈 | `./scripts/cargo-dev.sh check -p oasis7_proto`；测试示例 `./scripts/cargo-dev.sh test -p oasis7_proto <test-filter>` | 指定包/过滤器和开发配置；测试过滤器须核对实际非零执行，不据此声明 required/full |
| 显式本地诊断 | `./scripts/ci-tests.sh commit` | 当前 commit 清单；保留日志和未执行的 required/full 范围 |
| 正式测试 | `./scripts/ci-tests.sh required` 或 `./scripts/ci-tests.sh full` | 当前手册定义的适用 tier、实际平台和执行结果；仍需按改动补 UI/真实环境验收 |

统一入口使用 Python 3.11+；当前真实执行要求 Linux `/proc`，以便超时或取消后核实跨嵌套 session 的自有子进程已清理。选择 profile 后直接运行，不要求依次跑全部层次：

```bash
python3 scripts/check-local.py --profile quick
python3 scripts/check-local.py --profile affected --dry-run --format json
python3 scripts/check-local.py --profile affected
python3 scripts/check-local.py --profile formal --tier required
```

| Profile | 当前执行能力 | 明确边界 |
| --- | --- | --- |
| quick | 复用 `ci-tests.sh required --group baseline`；检查 committed、staged、unstaged 和非 ignored untracked 的空白问题及 baseline 已支持的工作树内容 | 不运行 Rust fmt/编译、业务测试、浏览器或服务；planner/policy 为 `not_used`。工具自测 case 与业务 case 分列 |
| affected | 从可信 base 加载同一 planner/config；将原计划交给 required driver，保留 case/target/features。支持干净提交，以及具有 snapshot 能力的可信 planner 所处理的 dirty/untracked 输入 | 已选组必须全部具备本地只读及执行证据合同，否则整次执行前 blocked，不删组后报通过 |
| formal required | 要求干净源码，显式 required，沿同一可信计划执行 canonical required 组 | 同样受当前本地组能力限制；不代替 hosted 或其他平台结果 |
| formal full | `--dry-run` 沿完整可信库存规划 | 真实执行 blocked：其余准备命令与平台组尚未完成本地适配；须补齐各组原义务，不能用 required 代替 |

当前已审计的组是 `baseline`、`codex_agent_config_validation`、`rust_baseline`、`consensus`、`distfs`、`node`、`net`、`pixel_world_bridge`、条件成立的 `oasis7_required` 和 `workspace_support`，以及 `doc_checker_contracts`、`cargo_tooling_contracts`、`compile_metrics`、`viewer_js_required`、`viewer_performance_report`。`oasis7_required` 若扩张到需要 Docker 构建 provider/builtin 资源的计划，也在启动前阻断。其余组，包括 Windows/macOS/fleet 和尚未接入结果证据的脚本组，保持 selected-but-blocked；具体原因见报告。该表是执行能力边界，不是另一套选测规则。

`workspace_support` 接受可信 version 1 的非空原生 profile 清单，仅执行未扩测的 targeted 计划；入口经 `--support-plan-file` 将完整原计划交回既有 driver，不重新选包或删除未适配项。当前 16 个可执行 profile 的写入限定于当前 worktree 的 ignored `target/` 或自有临时夹具，沿用 locked Cargo 与非零用例证据要求。工具和当前 Rust toolchain 必须预装，`oasis7_context` 另需 `/usr/bin/git` 和 `/usr/bin/python3`；不自动安装。包含 Worker/npm 准备、跨 UID 验收、尚未适配的真实 WASM target 或必要 ignored 用例的 profile，以及未知 profile、full 或扩大 features/targets 的计划，均在执行任何检查前整次 blocked，报告保留原 profile 清单和具体原因。

工程契约组三条完整 driver 清单分别有 11、10、3 项命令检查；缺少任何一项都不能通过。文档组将具有 `tomllib` 和 `markdown_it` 的同一 Python 绑定到整组及嵌套调用；过程式测试复用已有 Python contract 见证，分别核对完整入口、scenario 和实际断言，退出 0 或单独的 `OK` 文本不是完成证据。工具用例与命令检查分列，这三组的业务用例数为零。Cargo 工具组需要已有 Cargo / rustc 及 locked metadata；编译指标组还需要 x86_64 Linux、`zstdmt` 和带 `yaml/json` 的 Ruby，缺前置在整次执行前 blocked，必要 Python skip 保持 incomplete。工程 fixture 使用自己的范围和报告；编译指标的临时 worktree 登记放在自有 Git metadata 中。Linux 下既有 Windows-only fallback 不适用，可选 sccache 真缓存证明未被默认调用；报告明确保留这些未覆盖范围，不自动安装工具或用跳过代签验收。

Viewer 两组要求已准备好的 npm 依赖、Rust/WASM target、匹配锁文件版本的 wasm-bindgen CLI、agent-browser 和 Chromium；缺任何前置都在整次检查前 blocked，不安装或升级工具。源码契约、UI、生产构建、浏览器功能和认证检查沿原 driver 完整执行，产物写入本次报告下独有 Viewer 目录；浏览器只消费本次构建，现有 tracked bundle 不被改写或回退使用。UI 用 TAP-flat 核对完整正数清单，Node 用实际 TAP 计数，浏览器 suite 用完整运行的结构化回执；缺失、重复、跳过和部分结果不能通过，编排进程不重复计数。性能组保留 report-only 语义：完整采集后的阈值未达标会记录报告，采集失败仍失败，不把性能报告虚报为业务用例。浏览器配置、会话和 socket 使用隔离环境，沿已有进程监督收尾。

缺省基线只解析本地 `origin/main` 与 HEAD 的唯一 merge-base，不 fetch；`--base-ref` 必须为该信任锚的祖先。dirty 规划要求该 base 自身含 `LOCAL_INPUTS_VERSION=1`；旧 base 不支持时先 blocked，合入能力后更新本地 main 并 rebase，不能改用候选 planner。输入在私有对象目录中变成 tree，不创建 commit、操作用户 index/ref 或 checkout；base→HEAD、index、实际原始文件内容分别入图，保留抵消、删除、rename 两端和来源。显式 Cargo target/build/依赖、Rust 引用与 ignored 源码需要纳入读集或阻断。未启用 feature 可能引用尚未生成的夹具；入口记录其缺失身份，实际选中编译若需要它仍须失败，运行中出现该文件则 stale，不把缺失身份当成已验证字节。

- `--output-dir` 指定工作树外或 ignored 的报告父目录，每次创建独有子目录；不能放进 baseline 会扫描的 doc/scripts/skills 目录。省略时使用系统临时目录。text/JSON 和 `result.json` 来自同一对象，逐命令保存 argv/cwd、checker 身份、原始日志、耗时、退出码和证据数量；完成输入采集后同步附带只读 `workflow_advisory`（按重命名去重统计变更代码文件数、说明文档数、领先基线提交数及非阻断提醒，便于核对 30 文件建议、同域微改动合批与本地收敛后成批推送，不影响退出码或 `validated`）。
- `--timeout-seconds` 是每个 driver 组的时限，默认 2400 秒；`--planner-timeout-seconds` 默认 120 秒。有效检查、工具自测及业务 case 分列；无法解析的必要测试数量为 unknown/incomplete，零匹配和必要 skip 不会报通过。
- 不自动安装/升级工具，不格式化或暂存。Cargo 命令锁定依赖，保留当前 worktree target 和构建锁；外部/符号链接 target、编译器替换和未审计写入拒绝。允许 ignored 构建输出、自有临时/报告文件及锁定依赖与 advisory 缓存写入。`--offline` 禁止 Cargo 依赖联网；当前 RustSec 离线准备尚未适配，选中该组时在执行前阻断。
- 冲突、隐藏 index 标志、symlink 读集、Git 内容 filter/encoding、不支持的控制字符文件名、缺失/歧义基线和缺工具为 blocked（2）。运行中 source/index/ref/实际读集或 Cargo 配置变化为 stale_input（2）；失败、必要证据 unknown 或报告写失败为 failed/incomplete（1）；空计划为 no_checks（3）；中断为 cancelled（130）。显式 dry-run 只为 planned（0）、`validated=false`；全部必要检查实际完成才为 passed（0）。

旧 `ci-local-check-plan.py --profile affected --dry-run` 继续提供干净提交的兼容规划，始终不执行检查；其原有参数与证明范围不变。

`./scripts/fix-precommit.sh` 会执行 `cargo fmt --all`、`git add -u` 再运行 commit 诊断，是显式修改文件和 index 的修复动作，不能当作默认只读检查。

三个 profile 的完整合同见[共享设计](doc/testing/ci/ci-tiered-execution.design.md#local-check-entry-design)。未适配的 full/组/平台仍沿 #5386 承接，当前分片不关闭整体任务。按包开发 wrapper 和原显式 tier 仍可按各自原合同使用，不能把绕开统一入口的运行记作它已完成的只读验收。

<a id="local-check-entry-validation"></a>
### 本地入口核验（实施阶段）

Trace refs: O7-REQ-83ee480f-007c-446d-a05d-827c51b403e3；对应 [LOCAL-AC-1–5](doc/testing/ci/ci-test-coverage.prd.md#local-check-entry-requirement)。以下核验随实际开放能力分阶段完成，结果放在实施 PR；文档设计不构成运行证据。

**每片先验证共同底线。** 核对实际命令、读取范围、允许写集合和 text/JSON 同源结果。至少一项真实有效检查通过才可报告 passed；显式 dry-run 只有 planned/`validated=false`。对照[状态表](doc/testing/ci/ci-tiered-execution.design.md#local-check-entry-design)，注入本片适用的缺基线/歧义、冲突、输入不可读、缺工具、锁定依赖不可用、失败、预期测试零匹配、必要 skip、数量 unknown、空计划、取消、超时及报告写失败。未开放 profile/输入应 blocked，不能静默换范围。

核对执行前后 tracked/untracked 文件、index 与 ref；运行中改动须 stale_input。实际命令接线不得先写用户文件再回滚，取消只清理自有进程/临时目录。fake runner 用于调度与故障注入，每个开放执行 profile 还须有真实检查/消费者验证；没有业务测试的 quick 明示零业务测试，不虚构消费者覆盖。

| 交付范围 | 本阶段必要核验 | 留给后续的义务 |
| --- | --- | --- |
| quick | 真实调用 baseline，核对每项实际结果与工作树支持范围，特别检查传入 base/head 后是否遗漏 staged/unstaged/untracked；未调用 planner 明示 not_used，明确未执行 Rust fmt/编译/业务测试等类别；无法可靠处理的 dirty 输入 blocked | 不要求 Cargo 影响闭包或 dirty 精确计划，也不声称这些已覆盖 |
| affected：干净提交 | 临时仓库与现有可信 planner 同输入逐项比对；候选 policy 不能自豁免。只有 dry-run 时不得报执行通过；开放执行后用具有真实断言的小型消费者走 planner→driver→结果，核对 case/target/features 与非零执行数 | dirty 适配未完成时明确 blocked；准备性规划不算完整 affected |
| affected：dirty 精确输入 | 覆盖已提交、暂存、未暂存、未跟踪、删除/rename、特殊路径、同路径抵消、Cargo 清单/依赖变化和 ignored 显式依赖；逐项核对来源、选择理由、policy 与不可变视图，未知路径/解析不确定按可信规则扩张或阻断 | 本地选择与执行仍不代签 hosted、多平台或真实环境 |
| formal | 显式 required/full、干净 source、canonical 参数和必要组；dirty/untracked 阻断；记录实际数量、日志、缺环境与未覆盖平台 | 不依赖 dirty 精确适配，也不取消原 required/full 和真实环境验收 |

结果由入口生成，实施 PR 引用原始报告即可，不要求在 Issue、PR 和手册重复抄录同一份执行记录。每片完成后同步该片 help 与使用说明；最终按 LOCAL-AC-1–5 核对完整范围和真实证据，未完成项继续留在原任务。已有 CI 或其他正式验收只引用实际运行的版本与结果，不以本地报告替代。

## 本地 Cargo 开发约定

- 本地开发态 `cargo check/test/run/build` 优先使用 `./scripts/cargo-dev.sh <cargo-args...>`。wrapper 只调整开发 profile 与可用的 sccache，target 沿用 Cargo 原生解析，默认在当前 worktree 的 `target/`；不建立外部缓存 namespace 或符号链接，也不需要预先准备 target。直接 Cargo 与 wrapper 使用相同的 target 解析规则。
- 本地 smoke / playtest / prewarm / regression / drill / longrun 脚本若只是为了开发反馈，应 source `scripts/cargo-dev-lib.sh` 并调用 `oasis7_cargo_dev ...` / `oasis7_cargo_dev_debug_bin_dir`。`CI=1`、`CI=true` 或 `OASIS7_FORCE_RAW_CARGO=1` 跳过开发 profile，使用原始 Cargo。
- 该入口只服务开发态 profile 与编译加速，不替代本手册中的正式验收命令；手册里的 canonical 验收命令仍显式写原始 `env -u RUSTC_WRAPPER cargo ...`。
- 跨 worktree 的 registry 依赖编译结果复用由 sccache 提供，不共用 target artifacts。安装 `sccache`（macOS: `brew install sccache`）后，`cargo-dev.sh build/test/run` 自动启用它并关闭该调用的 Rust incremental；wrapper 不注入或覆盖 target 参数和环境。本地 lean 模式统一设置 dev/test debug=0，普通 dev/test profile 下 `cargo-dev.sh check/clippy ...` 启用 incremental=1 并清除 `RUSTC_WRAPPER`，以复用同一 worktree 内的增量检查状态；`build/test/run` 先选择实际缓存方式：启用 sccache 时 incremental=0，未安装或显式禁用时 incremental=1。Clippy 保留自身的 `RUSTC_WORKSPACE_WRAPPER`，不保证与 check 完整共用增量产物。显式 `--release/-r` 和非 dev/test 的 `--profile` 不使用自动 sccache，也不覆盖调用者的 profile/incremental 环境；`--` 后的参数不参与 profile 解析。sccache 执行失败直接返回错误，不切换模式重试。`OASIS7_CARGO_DEV_MODE=debug` 显式启用 debug=2、incremental=1 并关闭 sccache；不要在同一轮日常迭代反复切换模式，以免重编译。`CI=1` 或 `CI=true` 保留调用者原有的 profile/incremental 环境且不启用 sccache。`OASIS7_CARGO_SCCACHE=0` 可禁用自动 sccache，未安装时沿用 Cargo；该开关不改变本地 `check/clippy` 的增量策略。用 `sccache --show-stats` 查看命中；首次编译填充缓存，workspace/path crates 的源码路径仍进入 Rust cache key，最终链接和 proc-macro 不保证命中，`check` 不使用 sccache。
- 本地存储验收分阶段运行：先完成所需编译（可用 `env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib --no-run --locked`），再运行 `env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib --locked`；时限测试执行期间不要在其他 worktree 并行编译或施加资源压力。Cargo 的构建锁不保证测试运行期间独占主机资源。压力实验单独记录负载与实际截止结果，不通过延长测试 RPC 期限消除失败。依赖释放锁后立即重开的用例在独立测试进程运行，防止并行 libtest 的其他 spawn/fork 短暂继承锁描述符；子进程失败、超时和清理失败仍算失败。
- 磁盘有限时保留少量固定路径开发 worktree，串行任务在确认干净、独占且已保留未推送工作的工作区用 `git switch -c codex/<task> <base>` 切换独立分支；仅并行任务新建 worktree。不要编辑 main 或主工作区。`new-task-worktree.sh --with-harness` 是完整预热的显式选项，普通后端或文档任务不需要它。
- 工作区选择先检查 `git worktree list --porcelain`，不能仅看当前聊天附件。新增使用 `./scripts/new-task-worktree.sh <module> <task> --base <ref>`；根目录由 `git config --local --get oasis7.task-worktrees-root` 指定，未配置时采用脚本默认值。复用或外部创建的工作区也直接使用本地 target，无需准备链接。入口选择规则见开发规范。
- 盘点工作区磁盘占用使用 `./scripts/worktree-gc-report.sh --footprint`，只报告 Git 与文件系统事实。固定开发 worktree 的 target 可跨串行任务保留，由负责人按容量、闲置与后续用途显式回收；一次性任务专属产物按开发规范清理。关闭 incremental 不会自动删除已有产物。回收前停止相关构建和运行进程、核对路径及资料，再执行该工作区的 `env -u RUSTC_WRAPPER cargo clean`；显式外部 target 单独核对归属，全局 Cargo 下载缓存与 sccache 不随单个工作区清空。

- 最小开发反馈示例：`./scripts/cargo-dev.sh check -p oasis7_proto`；`./scripts/cargo-dev.sh test -p oasis7_proto <test-filter>`；需要完整交互时才运行 `./scripts/worktree-harness.sh up`。正式范围仍由本手册与 CI 决定。
- `runtime-module-routing-perf-harness.sh --profile dev` 与 `build-game-launcher-bundle.sh --profile dev` 使用开发 wrapper；bundle 的读取目录与编译 target 保持一致，默认玩家 bundle 不编译 operator binaries。默认 release 性能测量、packaging 和 wasm 构建保留原入口。
- deterministic wasm / release 链路继续保持 `CARGO_TARGET_DIR` 为空；涉及 `scripts/build-wasm-module.sh`、release evidence 或 hash/receipt 对账时，不改用开发 wrapper。Docker 使用工作区内的普通 target，不额外挂载外部缓存。

<a id="local-rust-feedback"></a>
### 本地 Rust 反馈与构建复用

按当前问题选择反馈，不要求每次修改顺序跑完所有层次。沿用[验证与评审](doc/engineering/workflow/source-of-truth.md#3-验证与评审)的局部自测和正式交付责任。

| 当前目的 | 选择的检查 | 证明范围 |
| --- | --- | --- |
| 接口、路径、泛型或生命周期修复 | `./scripts/cargo-dev.sh check -p <package>`；跨模块时包含真实消费方，测试接缝选择 `--tests` 或具名 `--test <target>` | 所选功能配置的类型检查；不执行行为断言 |
| 局部行为变化或已定位缺陷 | 对应 package/target 下能检出问题的定向回归 | 实际非零执行的用例；可复现缺陷按原规则先观察失败 |
| 模块路径已接通 | 整合负责人构建所需测试 target，再运行相关成功、拒绝与恢复用例 | 当前实际组合；各会话保留局部自测，避免重复构建同一重型组合 |
| 正式交付 | 适用的 canonical 命令、CI 与真实环境验收 | 本次交付所需范围；开发配置不代签正式结果 |

`cargo check` 跳过最终代码生成，不能覆盖所有 codegen 错误；测试名称过滤和 `--ignored` 主要改变执行范围，不保证减少所选大型测试程序的编译。`--no-run` 仅分离构建和执行。要减少构建成本，需核对 package/target/features 实际排除了哪些代码、夹具与依赖；`test_tier_required/full` 的名称不证明轻量，也不能通过关闭真实消费者需要的 feature 宣称该消费者已通过。参数语义见 [Cargo check](https://doc.rust-lang.org/cargo/commands/cargo-check.html) 与 [Cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html)。

同一轮迭代固定适用的工具链、profile/features/flags 和 worktree target，优先重复同配置 Cargo 命令，由 Cargo 核对产物是否仍有效。一次必要构建后可运行多组相关用例；源码、依赖/lock、工具链、配置或 build-script 输入变化时重新核对构建，运行时 fixture 变化时重跑相应断言。使用 `--no-run` 后直接执行产物时，从 Cargo `compiler-artifact` JSON 获取实际 executable，保留对应 cwd、环境和动态库搜索条件；无法可靠保留时通过 Cargo 执行，不从目录猜选旧程序。复用产物不等于复用旧测试结论。

跨工作区协调重型构建，时限或性能测试期间按上述存储验收规则隔离资源压力；等待 Cargo 锁时不换 target 强制冷构建。组合检查先消除接口错误，再在可执行行为形成时运行重型测试；必要回归不能因合批而延后到交付之后。

异常构建在下一次必要运行中附带 `--timings` 和所需资源观察，区分构建总时间、执行时间、退出码及实际用例数量。Cargo JSON 可提供 target、features、profile、executable 和 `fresh`，语义见[工具输出](https://doc.rust-lang.org/cargo/reference/external-tools.html#artifact-messages)；timings 的稳定报告供人阅读，不保证精确拆出锁等待、下载、codegen 和链接的全部耗时，未知项如实保留。引用已有工具报告和日志一次即可，不为计时额外造一次冷构建或新增手工流水。当前 `cargo-dev.sh` 不自动生成这些分段报告，未采集时不宣称已有数据；是否调整编译边界依据实际瓶颈判断。

### 旧工作区的 target 链接迁移

更新前已创建的 `target` 符号链接不会因代码更新自动消失。先用 `ls -ld target` 与 `readlink target` 核对目标，停止相关构建/运行并确认没有其他使用者。对已确认指向旧集中 Cargo 缓存的链接，使用 `unlink target` 只移除链接；后续 Cargo 自动创建普通 `target/`。已有普通目录无需迁移，不要对目录执行此命令。旧链接的外部产物在确认可重建、归属和无进程/其他工作区引用后单独删除；不能递归清空 `.oasis7-cache`、全局 Cargo 或 sccache 目录。脚本不自动改写或删除未知目标。

### ECS 缓存与完整 native 构建测量

ECS workspace 缓存的容量、信任边界和证据标准见[CI 设计](doc/testing/ci/ci-tiered-execution.design.md#ecs-workspace-artifact-cache)。`ecs-packages.yml` 先做只读容量预检，构建后重新清点；缓存故障继续完整构建。核对 `ecs-native-timings` 中的 preflight、stage、restore、save 及 unit 报告：配置预算不等于实时额度，action 命中不等于 payload 验证通过，`stage.save=true` 不等于远端已保存。两代为保留目标；容量足够时先保存第三代，再由独立维护确认当前条目并回收未保护旧代。容量不足应报告跳过，不能先删恢复点腾空间或提高额度。精确命中的成功 producer 也重新确认条目并触发维护。

手动执行 `ECS Server Package`，启用 `measure_builds` 并提供明确的 `measurement_baseline`，可同时取得正常完整包和独立的 full native/no-op/Viewer 改动对照。选择相同 jobs（串行归因选 1），保留 `ecs-build-metrics` 与 `oasis7-ecs-linux-x64` 两类产物。前者测量三个生产 binary 的完整 native 闭包，后者另验证 Web/native 组合与 package source/hash；不能互相代签。

已有所需 Git revisions 和 Linux x64/Rust 1.96.0 环境时，可直接执行：

```bash
python3 scripts/ci-ecs-build-metrics.py --baseline-ref <baseline-sha> --current-ref <current-sha> --jobs 1 --out-dir <external-new-report-directory>
```

脚本在自有临时 worktree/target 中预取锁定依赖，依次执行 seed、同命令 no-op 和固定 Viewer 注释变更重建。报告目录必须在 checkout 外，不能清空开发 target 造冷样本。核对来源、patch、profile/features/flags、Fresh/rebuilt 及实际重建闭包；比较不同功能修订时注明混杂因素。真实跨 run 加速仍需可信 main producer 保存成功和独立 consumer 恢复后的实际编译/包身份，PR 分支始终只读。具体测量协议见[性能设计](doc/testing/performance/compile-metrics-v2-warm-check.design.md#113-executable-ecs-native-build-and-viewer-experiment)。

## 当前实现分布（2026-02-18 基线）

### 应用主链（world + runtime + simulator + viewer 协议）
- 核心 crate：`crates/oasis7`
- 主要测试分布：
  - 运行时：`crates/oasis7_runtime/src/tests/*.rs`；状态/快照纯值与稀疏投影归 `crates/oasis7_world_state`，服务 DTO/具体 codec/证明适配归 `crates/oasis7_world_projection`。required Rust 测试必须实际执行两个新包的 lib harness，Runtime 的恢复、rollback、retention 与 Service 的 public/owner/delegate/撤权回归继续保留。
  - 模拟器：`crates/oasis7/src/simulator/tests/*.rs`
  - LLM 行为：`crates/oasis7/src/simulator/llm_agent/tests_followups_actions.rs`
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
- 场景定义：`crates/oasis7_simulator/src/scenario.rs`
- 场景矩阵设计：`doc/world-simulator/scenario/scenario-files.prd.md`
- 场景是 UI 闭环、协议闭环、压力回归的统一输入源。

## CI 现状与缺口（事实口径）

要求见 [PR 快速反馈与四小时完整回归](doc/testing/ci/ci-tiered-execution.prd.md#ci-fast-pr-periodic-full)。源码已接入快速 profile、完整库存、周期准入和发布结果消费；Rust workflow 每小时第 17/37/57 分钟检查是否到期，四小时是完整回归的启动目标；生效范围由实际合入版本、可信 BASE 与保护设置决定。hosted 完整覆盖、普通成功十分钟/P95 和四小时容量仍待实测，以下命令或本地 fixture 通过不代签这些结论。

<a id="ci-fast-pr-periodic-full-validation"></a>
### 快速 PR 与周期完整回归的实施验收

承接[分层设计](doc/testing/ci/ci-tiered-execution.design.md#ci-fast-pr-periodic-full-design)。先固定 source、实际 tested commit 与可信 policy，使用当前选择器、driver 和 workflow 核对下表。新 profile 仅在可信 BASE 支持时启用，旧 BASE 继续旧完整必要范围。

| 验收对象 | 方法与通过边界 |
| --- | --- |
| 普通文档/配置 PR | 只运行适用基础与增量关系检查，无关子检查及时结束；改动删除/重命名/索引时仍能发现失效关系；全库历史报告在周期层出现 |
| 普通 Rust/Web PR | 计划选择实际 crate/target/features 与必要消费者，保留适用正常/拒绝/边界/缺陷回归；日志证明编译闭包和行为用例均收窄，不能只用测试名过滤或零匹配证明提速 |
| 准入与默认归属 | 按[准入规则](doc/testing/ci/ci-tiered-execution.prd.md#ci-pr-fast-selection)对照纯文档、普通逻辑、接口拒绝及恢复变更，分别得到可解释的基线/影响/风险集合；新增、改名、漏标或无耗时历史的回归不会消失 |
| 候选变更测试分级 | required 改 full 必须在实际 owner/driver 上扩测；嵌套 cfg、跨 module 同名、缺 feature 声明、无扩展 driver 和新 ignore 均有负例。普通 case 删除/改名核对语义替代；Formal 的可信最低库存缺项必须失败 |
| 新增测试与辅助模块 | 沿实际 Cargo target 和静态父模块链证明普通组合可执行；`any(test, feature = "test-support")` 不因另一分支未启用而误拒绝。新增 native item 保留受影响包与风险范围；full 门控、未知宏、不可达父模块、禁用 harness、删除/改名混合与既有门控替换仍有保守或失败负例 |
| support 的扩展 target | 用普通 profile 只执行 library、且没有扩展 feature 的真实小包验证新增 integration：PR 选择扩展后及周期 full 都实际执行该用例；故意失败必须传播，不能因 feature 集合为空或只观察 library 通过而放行 |
| 重型 Formal 分层 | 普通 pr-fast/main-fast 在历史裁剪、crash/retry、共享控制或保守 scope=full 时也不启动重型 Formal；两条可信精确登记的 membership 长例不被 ordinary 批次重复执行，近邻及普通合同仍实际运行。periodic-full/candidate-full 保留五 suite、原场景长度及失败传播，旧 BASE 保留原选择 |
| 超预算与不稳定 | 构造 case 很短但编译很重、整体预计超预算及已有 flaky 的情形；必要集合不被静默裁剪，普通样本不在运行后改类，首次失败不被重试覆盖 |
| 关键边界与共享输入 | 身份/持久化/共识等命中必要定向加测；依赖/工具链或影响不确定时保守扩大并显示原因；未知路径仍失败 |
| 完整覆盖 | 对同源适用清单核对快速集合与迁出项均在 full 执行；保留原断言、场景长度和平台，包含 Clippy、独立浏览器安全 smoke、Command Center 及真实平台矩阵；故意漏项被识别 |
| 失败与产物 | 选中检查失败、取消、超时、缺产物、错 features/target、零匹配、意外 skipped 均使最终汇总失败；未选中与未执行严格区分 |
| 周期与并发 | 核对 schedule 的实际触发、准入判定、相邻完整回归的实际启动间隔、固定 SHA、完整回归独立并发、旧 PR 取消、漏跑/超时可见；main push 不重复触发全部重型回归 |
| 发布与恢复 | 同源 receipt 对应实际当前 run/attempt、成功完整 gate 和相同检查定义；错误 SHA、bootstrap、过期/歧义产物、失败重跑和 API 缺页阻止发布。Testnet 固定候选只可从可信 main 入口验证，原包 smoke 与授权回滚保留 |
| 反馈与容量 | 按 PRD 口径报告窗口与样本数、普通成功 P95、失败/超时/取消和例外、排队/编译/执行及冷暖缓存；完整回归能在周期内完成并保留 PR 容量 |

构造负例时复用现有 selector/result/driver 回归，不以 fake 构建代替真实消费者。实际 hosted 验收至少覆盖上述输入类别和冷缓存；有意注入失败只在隔离候选执行，不修改生产门禁。定时回归持续失败时先定位并验证修复，不能以换频率代替恢复。

### 当前计划与执行命令

workflow 将可信 BASE 的 planner、scope、结果汇总器和 Formal 库存放在同一 authority 目录。下面的变量代表这次已固定的输入；`CI_AUTHORITY` 不能指向候选自行修改的策略目录。

```bash
python3 "$CI_AUTHORITY/plan-rust-required-scope.py" \
  --profile pr-fast --base-ref "$BASE_SHA" --head-ref "$SOURCE_SHA" \
  --test-ref "$TESTED_SHA" --policy-ref "$BASE_SHA" \
  --formal-config "$CI_AUTHORITY/ci-formal-process-suites.json" --output "$PLAN_PATH"
./scripts/ci-tests.sh required --group oasis7_required --plan "$PLAN_PATH"
```


可信 `fast_test_inventory_version=1` 启用时，native 构建与 strict Clippy 共同消费 `ordinary_fast_targets`。最低 case 必须存在且非 ignored，但目标内所有非 ignored 用例都执行；便携 WASM 范围保持。`required-args --phase native-commands` 输出实际命令列表。完整 profile 保留原库存，并以 `ordinary_full_fast_targets` 追加快速目标的精确无默认 feature 组合；不同 feature 的同名 harness 独立打包和执行。

本地入口（增量物理 fixture 适用于 native Unix，canonical PR runner 为 Linux）：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --test pr_fast_runtime \
  --no-default-features --features test_tier_required --locked
env -u RUSTC_WRAPPER cargo test -p oasis7_world_state --test pr_fast_reducers \
  --no-default-features --features test_tier_required --locked
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --test pr_fast_incremental \
  --no-default-features --features test_tier_required --locked
```

验收包括旧 BASE/控制自改、未映射与混合风险、测试 gate 正文变化、声明删除替换、共享新增 case 的实际 target 可达性，以及真实 Cargo fixture 对未选重目标的编译失败注入。artifact 缺目标/最低 case/feature 变体、零执行和 Clippy 失败均须传播；完整模式仍保留旧 ordinary、五个 Formal suites 与平台集合。上述本地命令和 fixture 不代签 hosted 十分钟或跨平台 full 成功。

仅在计划选中 `oasis7_required` 时运行该组；选中的 package 为空会失败。未启用独立 target 时，该组按计划收窄 native `--tests`、Clippy 和便携 WASM 包，实际执行全部适用非 ignored 测试。support 以 `--support-plan-file "$PLAN_PATH"` 读取完整计划，Formal 的 `ci-test-artifacts.py build/pack/run` 使用同一 `--plan` 和 `--formal-config`；producer/consumer 计划、source 或 authority 不符会失败。完整计划及逐路径原因保存在 selection artifact，workflow 输出仅用于调度；执行和最终门禁都读取完整文件，避免大批量改动触发环境变量或进程参数长度限制。

`main-fast` 使用 push 的完整差异；`periodic-full`/`candidate-full` 无须 diff，直接从可信库存展开全部组。完整 hosted 图并行执行普通组、Formal、平台和 `./scripts/ci-tests.sh full-extended`，最后由 `explicit-full-gate` 核齐。`full-extended` 单独成功不代表完整回归，本地 `full`/`full-standard` 也不能代签其他 OS 的结果。

发布候选通过 main 上的可信入口固定版本：

```bash
gh workflow run rust.yml --ref main -f run_mode=full -f source_ref="$SOURCE_SHA"
```

不传 `source_ref` 时验证所选 main 事件的固定 SHA。非 main 的 `--ref <candidate-branch-or-tag>` 仍可只读验证接线；可信 main 尚无新 profile 时，该首次运行使用旧完整库存并标记 `bootstrap_full`，不能取得发布资格。所有 manual full 都不能替代 PR 必需事件的检查。

发布消费者使用 `ci-full-result.py verify --source <sha>`，默认完整查询最近 30 天相关 main full runs，并消费保留 30 天的准确 attempt receipt。`--run-id` 可限定准确运行，仍核对其当前 attempt、固定 source/tested/policy、检查定义及真实完整 gate；较新同源运行缺当前成功结果（含 pending/running）或失败重跑不会自动退回旧绿色。Testnet 的 `--allow-candidate-source` 只允许 main 上的 `testnet-packages.yml` 使用，保留固定候选来源，不免除完整验证；Release/Mainnet/ECS 继续要求原 main 来源合同。

主线健康只读检查使用已获准的 `GH_TOKEN`/`GITHUB_TOKEN`，不在命令中打印凭据：

```bash
GITHUB_REPOSITORY=nil-studio/oasis7 python3 scripts/ci-full-result.py health
```

它观察最近两天 schedule 的实际 attempt，并回看旧 run metadata 与全部活动轮，通过实际 jobs 和 admission 产物排除合法轻量空跑，报告最近实际 full、最近成功 SHA 与提交区间。空跑不会清除先前 full 失败或更新成功 SHA；缺失/错身份/冲突的 admission 保留未知。每二十分钟观察只增加补偿机会，实际 full 启动间隔仍须从 Actions 验收。五小时未启动、未完成轮超四小时、无最近成功或失败均返回非零；新 pending/running 不清除上一终态失败。该命令不是发布 qualification，也不代表外部定时巡检已启用，既有 CI 回顾任务按实际授权调用。

### 当前 CI/脚本已覆盖
- 入口 A：`scripts/ci-tests.sh`（主流程）
- `commit`：
  - `./scripts/doc-governance-check.sh`
  - `./scripts/check-script-executable-bits.sh`
  - `cargo test -p oasis7_consensus --lib`
  - `cargo test -p oasis7_distfs --lib --bin oasis7_durability_endpoint -- --test-threads=1`
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
    - 这是该 package 的必要 feature 入口；hosted ordinary 实际按计划选择 package，并保留全部适用非 ignored 测试。runtime builtin wasm 扩展用例保留在 full，命中必要风险或分级迁移时仍在 PR 加测；标签本身不证明轻量。
  - `cargo test -p oasis7_consensus --lib`
  - `cargo test -p oasis7_distfs --lib --bin oasis7_durability_endpoint -- --test-threads=1`
    - 具名 `tests/incremental_industry_jobs.rs` 迁入后，driver 追加默认 features 的 `cargo test -p oasis7_distfs --test incremental_industry_jobs -- --test-threads=1` 及该目标的严格 Clippy；同一可信计划的精确快速库存未选择行业目标时，两者都不追加，以保持既有快速路径覆盖。无精确库存、策略变化、full 或独立本地组仍执行；行业目标及 support 的直接或混合变化必须保留行业覆盖。行业选测不由 library Clippy 的转交 owner 决定，执行时仍归 DistFS，非法计划和 helper 失败必须传播。Formal 独立产物继续只构建 library。新目标必须先由可信 BASE 的 planner 证明真实执行根与完整模块门控。
  - `npm --prefix crates/oasis7_viewer run test:feedback-contract`
  - `npm --prefix crates/oasis7_viewer run test:ui`
  - `npm --prefix crates/oasis7_viewer run build:software-safe`
  - 变更 `oasis7_game_client` 时，`workspace_support` 运行独立包的全部 targets strict Clippy 和库测试，并沿 Cargo 依赖图保留主应用消费者。CLI 可用 `cargo run -p oasis7_game_client --bin oasis7_game_client -- --help` 单独验证。
  - 变更 `oasis7_agent_api` 或 `oasis7_agent_harness` 时，planner 选择 `workspace_support` 内实际受影响的 package profiles：API wire-contract 集成测试、Harness 单元测试及对应 strict Clippy；Cargo 依赖图仍保留 `oasis7` 消费者组。Launcher 代码变更保留 native profile 与 `launcher_web`。支持包清单位于 `plan-rust-required-scope.py`；驱动以 `--support-plan-file` 读取 trusted selector 的完整 JSON 文件，旧 `--support-plan` JSON 参数继续兼容。文件路径一直传到解析器，不将完整 JSON 插入 shell 源码、环境或后续进程参数。
  - support profile 计划版本为 1；空、未知、重复 profile 或未知版本拒绝，保守 full scope 展开完整库存。旧 trusted planner 没有 profile 版本字段时仍执行完整支持包库存，不提前缩测；控制/manifest/lockfile/影响不确定及非包规则保守扩大，已确认的未登记路径明确失败。每个 profile 的命令成功后才输出完成标记，失败仍由 `required-gate` 聚合。直接本地 `required --group workspace_support` 不传计划时保留原完整入口。
  - planner 始终声明 `baseline`：这是 required-gate 的普适治理/契约入口，不等同于每个 Rust、Node、provider 或 P2P 套件都对所有路径执行。baseline 保留受影响文档、反向引用及必要全局不变量；`--full-corpus` 历史报告按需显式运行。规则、范围选择或解析器改变时运行全部适用治理回归，并核对完整适用文档集合。
  - 触达场景定义、场景 integration test 或场景 PRD 时，planner 追加 `scenario_regression`，运行窄命令：`env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --test oasis7_init_demo --features test_tier_full oasis7_init_demo_runs_`；不追加 test-harness 参数，保证 `CI_VERBOSE=1` 的 Cargo verbosity 仍由 wrapper 正确处理。
  - provider remote-HTTPS、P2P/testnet、state-sync、S10 与 release-preflight contract 只在 planner 选择 `operational_contracts` 时运行；对应跨平台 workflow jobs 未命中时直接 skip，保留 stable context 的同时避免 docs-only 或不相关 PR 分配 runner 执行跨域 operational work。
- `full`：
  - 调用 `run_full_required_superset`、`rust_baseline` 及 full 扩展，另加 `formal_process`；`full-standard` 保留普通与扩展部分。
  - 已补齐 `oasis7` / consensus / distfs / node / net 的组级 Clippy（net 默认启用 libp2p）、默认 snapshot 回归，以及依赖本轮实际构建的 `viewer-auth-browser-security-smoke.mjs`；workspace-support 的原检查保留。
  - hosted 完整覆盖由全部配置组、平台矩阵和 full 扩展共同提供，不能仅凭本地函数名或单个 `full` 命令通过推断跨平台完整结果。
  - `cargo test -p oasis7 -p oasis7_viewer_runtime --tests --features test_tier_full,wasmtime,viewer_live_integration`
  - `cargo test -p oasis7_node --lib`
  - `cargo test -p oasis7_net --lib`（默认启用 `libp2p` 和 `self_tests`）；vendor 和 WASM 检查保留各自入口。
  - `./scripts/llm-baseline-fixture-smoke.sh`
  - `run_workspace_support_full_feature_tests` 按实际 manifest 对适用 support 包执行 required/full/wasmtime/viewer 等完整 feature 并集和 `--all-targets`，与原 profile 共同保留首次 PR 之后的周期覆盖。
  - 构建真实生产 `oasis7_chain_runtime` / `oasis7_game_launcher`：Chain 所属包启用 `wasmtime`，Launcher 使用 `oasis7_launcher_backend` 的默认 features，两者均不含 test-tier feature。`scripts/hosted-local-mock-production-guard.py` 按所属包核对 Cargo artifact 清单中的 manifest、bin target、非 test harness 与各自的 feature 要求，再分别启动并核对 Hosted local-mock funding 参数被生产 guard 精确拒绝；不以其他启动错误代替拒绝结果。
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
  - `full` 可显式本地运行。Rust workflow 的 schedule 为 UTC `17,37,57 * * * *`，每 20 分钟观察一次。准入以最近一次实际 full 启动为准，满四小时且无活动 full 才启动完整回归；四小时是目标，不是实际触发间隔保证。完整 profile 展开全部配置组和平台，专项环境仅在普通 CI 存在具体不足时使用。
  - `pr-fast` 只读消费缓存，`main-fast` 默认只读。只有可信计划明确启用写入能力、workflow/source/tested 均绑定当前 main push 且普通产物 producer 采用完整 ordinary 范围时，才可按预算保存 ordinary workspace library 缓存。可信 main 的完整 producer 保留原预算内写入，旧 BASE 回退保留原权限分支。候选完整回归缓存只读、不写 trusted sentinel。
  - select summary 记录 source/tested/policy 三个 SHA，candidate-full 另记录实际 tree；artifact 绑定 tested commit 与真实构建输入。main manual full 可固定不同于 workflow 事件的 candidate source。它验证所选源版本，不自动代表最新 main 合并组合，必须核对 `explicit-full-gate` 的所有实际依赖，而非单个扩展 job 或旧运行。
- 入口 C：`.github/workflows/wasm-determinism-gate.yml`（构建 hash / receipt evidence 独立 gate）
  - GitHub-hosted runner 矩阵：`(m1|m4|m5) x (ubuntu-24.04/linux-x86_64)`
  - planner 先执行：`./scripts/plan-wasm-determinism-scope.sh --event-name <push|pull_request|workflow_dispatch> --base-ref <base> --head-ref <head>`
  - 仅命中的 module set 会实际执行：`./scripts/ci-m1-wasm-summary.sh --module-set <m1|m4|m5> --runner-label ... --out ...`
  - planner 校验后输出同源 `selected_module_sets_json`；collect/verify 在 job 层过滤 skip，并只展开选中模块。无关改动只运行 planner，单模块共三个 runner jobs，全部模块共七个。planner、collect 或 verify 失败均不能算通过。
  - Wasm workflow 按 workflow 和 PR 编号取消过期运行；非 PR 使用独立 run_id。只有后继匹配事件触发运行时才取消旧运行。
  - verify job 会按命中的 `module_set` 下载 summaries，并执行：`./scripts/wasm-release-evidence-report.sh --module-sets <m1|m4|m5> --skip-collect --summary-import-dir <downloaded-summary-dir> --expected-runners linux-x86_64`
  - verify job 同时上传 `summary.md/json + logs + module_sets.tsv` 的 release evidence report artifact
  - collect 失败时另保留 `wasm-candidate-raw-<module_set>-<runner>-<run_id>-<attempt>` artifact，保存 7 天，只包含实际生成的 `.wasm`、`.metadata.json` 和 `.build-receipt.json`；构建早期失败时可能不完整，上传成功不改变原 gate 的失败结果。成功运行不重复上传这些诊断原件。
  - `docker/wasm-builder/Cargo.lock` 等 producer 输入改变后，即使 WASM hash 未变，candidate identity 仍需刷新。从同一实际源码与 recipe 的三组运行取得全部 37 组原件，保留原 bytes，拒绝路径穿越、重复文件和覆盖；在匹配源码的本地工作区按[候选基线生成约定](doc/world-runtime/wasm/wasm-deterministic-build-pipeline.design.md)运行 `OASIS7_WASM_SYNC_WRITE_ALLOW=local-dev env -u RUSTC_WRAPPER cargo run --locked -p oasis7_distfs --features sync-builtin-wasm-identity --bin sync_builtin_wasm_identity -- --candidate-baseline-prepare "$PWD" <raw-dir> <new-staging-dir>`，审核 staging 后提升 candidate 文件，再对 m1/m4/m5 执行 `--candidate-baseline-check "$PWD" <raw-dir> <module-set>`。CI 不生成或提交基线；缺原件不得用日志哈希补造 receipt，也不修改冻结的 release 清单。
  - 若要补独立跨宿主产物证据，可把外部 Docker-capable macOS runner 产出的 summary 作为额外 import 输入，再以 `--expected-runners linux-x86_64,darwin-arm64` 做离线对账；该结果不属于 `test_tier_full`，也不替代 `./scripts/ci-tests.sh full`

### CI 工具缓存与 local-signer 验证

- Viewer 的 `viewer-js-required`、`viewer-performance-report` 只恢复当前 Cargo.lock 版本的 wasm-bindgen CLI 目录；key 包含 OS、架构、Rust 版本和两个安装 helper 的 hash，不使用宽松恢复。精确命中必须验证目录内 binary，损坏命中直接失败；冷目录以 `--ensure-cache --print-bin` 安装并验证。普通 PR 不保存该工具缓存。
- 现有 main schedule/manual-full `full-regression` 是唯一工具 writer，准备成功后立即保存，再运行 full tests。Rust 依赖缓存由独立 `rust-cache.yml/shared-native-cache` 的 main schedule/manual-full 生产，身份为 `ci-shared-native-trusted-v4`；完整测试只读该缓存。首次写入与后续自然 Viewer PR 的热命中需分别从日志核实；热路径应无 CLI 源码安装，缓存成功不代表 full 回归通过。
- 共享依赖预热的验证：运行 `python3 scripts/ci-rust-cache-build.test.py`、`python3 scripts/ci-rust-cache-admission.test.py`、`python3 scripts/ci-cache-maintenance.test.py` 和 `bash scripts/package-workflow-cache-reuse-contract.test.sh`，核对独立 producer 的全部 build-only 组合、精确 fast features、首编译失败停止、真实 checkout 身份及 v3 退役保护。精确 v4 hit 只省去重复预热，前缀 hit 或 miss 必须完整编译。
- hosted 分别核对 Rust full 的全部选中结果与独立缓存 run 的准备、编译、实际保存；缓存 run 不产 full receipt。完整回归保持原成功/失败/取消判定及发布消费者的整条 run 成功要求，不能把缓存成功算成测试通过。补齐 fast 依赖后的普通 PR 收益须包含首次 v4 冷恢复、后续实际命中和测试执行成本。
- Linux packaging 与 full 通过 discovery 运行 local-signer 完整 suite：`python3 scripts/ci-local-signer-tests.py`。只允许已知 Darwin descriptor 项 skip；零测试、缺 native 项、导入错误、其他 skip 或失败均返回非零。
- fleet-health 的 Linux/Windows/macOS 原生分片执行 `env -u RUSTC_WRAPPER cargo test --locked -p oasis7_hosted_access`，覆盖永久 OS 锁、双 stale 回收者、崩溃释放与 nonce 原件并发消费；仅适用于受信本地文件系统，不证明网络共享盘或多机互斥。该包的修改选中 fleet-health。
- fleet-health 的 macOS 分片执行 `TMPDIR=/private/tmp python3 scripts/ci-local-signer-tests.py --require-darwin`，native descriptor 必须真实成功且整个 suite 不得 skip。 Docker/Linux packaging 仍只跑 Python。macOS 还须在已准备固定 Rust 工具链后，由安装器构建器生成私有临时夹具，并真正执行唯一的 Rust installer boundary importer；零执行不算通过。测试目录与 local-file-signing runbook 选择 packaging/fleet；四个 Python 生产入口另选 workspace_support；未知生产模块继续 full。
- CI 控制改动的 PR 仍由可信 base selector 选择全部适用组。候选 selector 的精确路由仅在合入后成为可信规则，required-gate 的组集合与失败判定保留。
- 本地合同验证：`bash scripts/plan-wasm-determinism-scope.test.sh`、`bash scripts/ensure-wasm-bindgen-cli.test.sh`、`python3 scripts/ci-local-signer-tests.test.py`、`python3 scripts/plan-rust-required-scope.test.py`、`python3 scripts/ci-required-result.test.py`、`python3 scripts/ci-workflow.test.py`；前三项接入 workflow_governance。
- net 的 libp2p/Python/WASM 检查、Viewer auth smoke 与 performance visual build 均通过 `ci-tests.sh --group` 执行一次；YAML 负责工具、浏览器和依赖准备。stable 安装使用 minimal 并显式安装 rustfmt/clippy，nightly、rust-src 与 WASM targets 保留。

<a id="ci-formal-process-validation"></a>
### 真实进程分片与产物复用验证

要求与机制见[真实进程验收要求](doc/testing/ci/ci-tiered-execution.prd.md#ci-formal-process)和[调度设计](doc/testing/ci/ci-tiered-execution.design.md#ci-formal-process-design)。

- 可信 BASE 启用 `formal_full_profiles_only_version=1` 后，普通 `pr-fast/main-fast` 不运行重型 Formal producer 或分片，即使 `scope=full`、命中关键风险或追加 `formal_process`。`periodic-full` 与显式 `candidate-full` 必须执行全部五个逻辑 suite；CAS/Formal Protocol 普通库、单元及 CI 合同测试继续按影响选择。旧 BASE 缺能力时仍按旧规则，策略 PR 自身可能仍运行 Formal。
- 核对 ordinary 日志中的 `ordinary_formal_deferred`：只后移可信计划精确列出的两条 membership 长例，不按 `formal` 名称批量过滤。真实 libtest 回归验证近邻仍执行、普通失败仍阻断，full/旧 BASE 仍执行延期 case 并传播其失败；完整库存回归核对两 case 仍各归属一个实际 Formal 分片。
- `./scripts/ci-tests.sh required`：普通检查，不隐式启动独立重型链路。
- `./scripts/ci-tests.sh required --group formal_process`：准备产物后顺序执行全部真实进程分片；兼容入口 `./scripts/formal-live-local-tests.sh "$PWD"` 调用相同清单和运行器。
- `./scripts/ci-tests.sh full`：full-standard 加重型验收。本地 `full-standard` 执行普通与扩展检查；hosted periodic/manual full 分别启动全部普通组、`full-extended` 与共享 Formal 产物矩阵，不重复调用整套普通入口。
- 合同回归：`python3 scripts/ci-formal-process.test.py`、`python3 scripts/plan-rust-required-scope.test.py`、`python3 scripts/ci-required-result.test.py`、`python3 scripts/ci-workflow.test.py`。使用 Python 3.11+；覆盖精确零匹配、清单遗漏/重复、旧基线扩大、选中后失败/取消/跳过、绝对期限和真实小型 libtest。
- hosted 验证检查构建与各消费 job 位于不同 Linux runner：下载同 tested SHA 的消费者 tar.gz、核对 manifest 与所选执行闭包、无本地 target 前置时运行全部真实用例。不得在分片调用 Cargo。普通组与正式矩阵分别依赖各自 producer，正式矩阵不等待普通构建；互斥正式 producer 恰有一个成功且另一个跳过，任一必选分片失败则固定汇总失败。
- Formal build/pack/run 使用同一可信 `--formal-config`、计划与 source；失败任务重跑可复用实际成功 select/producer 的先前 attempt，产物按真实 producer attempt 核验，不要求三步都发生在当前 attempt。BASE 最低 case/ignored filter/batch/script 与候选新增项取并集，manifest 绑定 authority 摘要。删除候选条目不能消除旧预期 case，缺 case、身份变化、计划外物理 shard 或 authority 不符必须失败；普通测试的语义退役仍按既有评审处理。
- 分路构建回归用真实微型 Cargo workspace 完成 `formal-debug`、`formal-endpoint`、`merge-formal`、完整分包和下载后执行，核对 debug opt0 保留断言/溢出检查，旧 opt2 Formal 组件与完整 bundle 在删除或改变执行版本标记后仍被拒绝，storage 专项独立保持 opt2、endpoint release opt3。缺目标、重复身份、角色互换、跨 run/source/计划/authority、未来 attempt、篡改文件和部分 bundle 直接 pack/run 必须失败；同 run 较早成功 attempt 可复用。工作流枚举两个组件与两合并入口的成功/失败/取消/跳过组合，合并失败不能继续分包。
- 构建诊断回归执行 `python3 scripts/package-ecs-resource-monitor.test.py` 和 Formal 真实微型 Cargo fixture：按 variant 检查 `--timings`、开始/结束摘要及每 20 秒的观察心跳，保留 CPU/RSS、内存和 swap 的实际测量边界。失败退出仍阻断后续构建和产物消费，观察器失败不能改变 Cargo 结果；已存在的旧 timing 不进入本轮诊断，两个组件均在失败后上传已产生的安全报告。hosted 读取 `formal-debug-build-diagnostics-*` / `formal-endpoint-build-diagnostics-*`，缺失数据记为未知，不将心跳当作编译进度或性能验收。
- 执行池回归使用真实子进程屏障核验 CPU 槽、worker 上限、独占项与旧产物串行；候选配置不得增加 BASE 并发许可。并发 case 的非零退出、错误计数、超时、取消与遗留子进程均保留失败，排队项取消后不启动；已完成成功结果保持原身份。hosted 分别比较构建两路、最长完整 case 与分片总体耗时，不能把并发宽度当成已实现收益。
- 优化回归核对全部完整用例在逻辑 suite 与物理 shard 中恰好一次，保留长历史 70 次及健康集合 40 次连续操作。比较 producer、分包原始/压缩 bytes、下载、实际测试、重开/冷查/stop 各阶段；记录缓存来源、是否命中及 8 GiB 保存预算结果。fixture 和本地计数器只能证明对应机制，整体耗时改善须用实际 runner 结果另证。
- workspace 缓存回归使用真实小型 Cargo workspace：分开保存依赖与本地编译单位，在相同绝对路径重建 checkout 后恢复，核对 Cargo JSON 的 fresh 和实际程序输出。修改源码、build.rs 观察的未跟踪输入、权限或刻意保留旧 mtime 的输入须正常重编；新增/删除输入、损坏、symlink、路径穿越和不完整缓存须安全回到冷路径。核对 build-script 编译与执行两类目录均完整，原始 fingerprint 不被修改。真实 producer 另记录完整 target 与所选 workspace 的逻辑/硬链接去重 bytes、目录归因、8 GiB 保存结果及两类缓存的恢复耗时；本地 fixture 不代签 main writer 已保存或后续 hosted reader 已复用。
- `output/formal-process/results.jsonl` 记录每个命令的开始时间、耗时、退出码、超时、测试数和清理状态。只上传此安全摘要；详细输出查看 job 日志，失败世界目录不自动上传。缺摘要、缺产物和构建失败须从 Actions 失败步骤诊断，不能计为通过。

### Workspace 缓存容量和滚动验收

- 本地合同：`python3 scripts/ci-formal-process.test.py`、`python3 scripts/ci-cache-policy.test.py`、`python3 scripts/ci-workspace-cache.test.py`、`python3 scripts/ci-ecs-cache.test.py`、`python3 scripts/ci-cache-maintenance.test.py`、`python3 scripts/ci-workflow.test.py`。实际 Cargo fixture 需要 Rust；缺少工具链时明确记录跳过，不能用 synthetic JSON 计数代签真实 fresh。
- 普通构建 library-only fixture 须证明库和完整构建脚本可复用、普通 binary/test harness 不入缓存且正常重建；四个 feature variants 与测试产物不变。新增/删除输入、损坏或路径不安全继续 miss；源码内容变化仍由 Cargo 重编。检查本地与第三方 fresh/rebuilt 分组，含恢复、验证和打包成本。
- 各 workspace writer 均核对完整库存、实际额度/配置预算来源、压缩归档及外包装上传上界。单份上传、两代常驻和第三代临时容量分别校验；普通原始 staging 须覆盖 4,428,645,549 字节实测选集。两代加足够容量允许临时第三代；保存准入本身始终零删除，通常维护在库存不全、未确认 current 或族归属不符时零删除。启动维护仅回收明确退役格式的冗余代，每个兼容前缀/cache version 组保留完整恢复锚点，并覆盖锚点歧义、两次清点间变化和活跃保护。回收保护 current、前代、活跃读写和零字节条目；两次清点只缩小计划。Rust 维护直接等待普通与两路 Formal 的可信 writer，不再等待完整测试；ECS 维护等待 native，精确命中也可执行；维护结果不能改变 required/full 结论。
- Formal debug/endpoint 分别核验 6/2 GiB staging 与 768/256 MiB 单份上传上界、独立 writer 身份和兼容 key。新两族合计两代目标 2 GiB、临时第三代 3 GiB；旧 Formal v3 仍独立受控，迁移期间必须计入完整仓库占用，不以一半成功跨族删除另一半或历史恢复锚点。
- 两路依赖缓存另核对 debug opt0 与 endpoint release 的精确 namespace/profile。真实组件的 main schedule/manual-full 成功 job 才能保存，候选、错 job/key/平台、非 full dispatch、旧源码和归档容量不足均拒绝。首次 seed 后分别观察对应 hosted reader 的第三方 fresh/rebuilt；旧 shared-native 命中不能代签新 profile 的预热。维护回归保留三者各自完整恢复锚点，上传准入与维护均不改变测试结论。
- compact archive 回归覆盖 zstd 与旧 gzip 往返、真实同 inode 去重、完整逻辑字节预算、损坏/截断/附加帧、非法别名和路径拒绝。使用实测 9,381,351,049 字节库存与有来源的大小样本回放 bootstrap、首次准入、后续恢复和第三代轮换；全族达到上限但仓库空间不足时必须拒绝，不能把单份上限当预留额度。
- hosted 分别核对首次 main 保存、后续兼容 reader 的实际 fresh 和第三代回收后的库存；新格式首次冷构建、超限跳过与保护集超额均如实报告，不承诺尚未测量的耗时收益。

<a id="ci-dependency-installation-validation"></a>
### CI 依赖安装与 Release 工具缓存核验

要求与设计见 [CI 分级执行 PRD](doc/testing/ci/ci-tiered-execution.prd.md#ci-install-trunk)和[依赖准备设计](doc/testing/ci/ci-tiered-execution.design.md#ci-install-trunk-design)。本节给出验证方法；具体执行版本、结果和未覆盖范围记录在对应 PR 与 Actions 运行中。

- 本地接线回归：`bash scripts/release-packages-trunk-cache-contract.test.sh`。检查 Release 使用计划源中的共享安装器、与 Rust CI/main writer 一致的归档身份、只恢复 action 和冷热均执行验证，必要失败不能变成成功。历史 tag 缺安装器时应在恢复前明确失败。
- 安装器行为：`bash scripts/install-ci-trunk.test.sh` 覆盖冷下载、合法缓存、损坏缓存/下载、错误版本、错误 pin 及受支持宿主的真实 binary。此测试首次需要下载固定官方归档并核验摘要；缺网络时如实记录未执行，不能以离线接线测试代替。损坏归档不得先解压或执行。
- hosted 功能：读取后续自然发生且已授权的 Release `build-web-dist`，确认工具验证后原有 Web 构建、source/BUILDINFO 与发布检查正常。不要为本节人为触发发布；未发生的发布保持待观察。
- 真实跨 tag 缓存：关联 main `full-regression` 的实际归档保存记录和两个不同 tag 的 reader，记录 run/job URL、event/ref、请求/匹配 key、工具/安装器身份、路径及 cache version 兼容条件、恢复和摘要验证结果。同 tag rerun、仅配置相同或只有 cache-hit 输出均不足以确认 main seed 复用。
- 冷暖量测：区分实际下载、跨 run 恢复、本地已有和恢复异常后下载；分别记录恢复、下载、验证安装、总准备及 writer 保存成本。日志只有 step 级时间时保持该粒度。附 runner image/OS/arch、source、Trunk 版本/摘要、helper 指纹、样本窗口与数量；失败和取消单列，排队和整个 Release 耗时另报。样本不足时报告原始观察，不预写提速百分比。
- apt 本地合同：`python3 scripts/ci-apt-install.test.py`、`python3 scripts/ci-workflow.test.py`、`python3 scripts/plan-rust-required-scope.test.py`。假命令与临时进程 fixture 验证正常、瞬态/永久/未知错误、重试耗尽、总预算、挂起或忽略终止信号、父进程先退出、取消与受控进程清理；不得在真实 apt/dpkg 上故障注入。接线测试核对全部原包列表、lsof 已有时短路、600 秒 helper 参数、11 分钟步骤兜底，以及失败后不运行依赖测试。
- apt hosted 验收：在新提交实际运行中分别检查 update/install 阶段耗时、尝试次数、退出/清理结果与原测试结果；记录 source SHA 和 run/job 链接。600 秒仅为 Rust/native cache 本次参数，可据后续样本调整，不代表全局 SLO 或已经 hosted 验收。强制终止后的包状态不明确时应失败并使用新 runner；helper 失败保持沿原 job/required 汇总传播。

Trunk 的本地合同测试只证明其实际范围；main 合入、hosted 构建、跨 tag 恢复与性能收益分别核实，不为 Trunk 新增通用 required check、预热工作流或验收回执。

<a id="engineering-telemetry-validation"></a>
### 工程遥测报告核验

Trace refs: O7-REQ-7a766dd0-5264-4caf-9dfb-60643ca5faf6；要求与模型见 [工程遥测](doc/testing/ci/ci-tiered-execution.prd.md#engineering-telemetry)及[共享事件设计](doc/testing/ci/ci-tiered-execution.design.md#engineering-telemetry-design)。当前提供离线 v1 投影、v2 来源/身份合同与 DAG 报告，以及有界 Actions/既有指标适配；以下分别验证离线合同和真实接入，不以合成样例代签真实运行。

从仓库根目录运行：

```sh
python3 scripts/engineering_telemetry.py collect scripts/tests/fixtures/engineering-telemetry/input.json /tmp/telemetry-snapshot.json
python3 scripts/engineering_telemetry.py report /tmp/telemetry-snapshot.json /tmp/telemetry-report.json /tmp/telemetry-report.md
python3 -m unittest discover -s scripts/tests -p 'test_engineering_telemetry*.py'
```

v2 完整合成输入的 collect 应返回 0；缺失/冲突或 v1 输入保留 partial 并返回 1。report 成功渲染返回 0 不等于测试执行成功；核对原 execution_result、退出码与 cleanup_result 未变。输入与输出格式见 [fixture 说明](scripts/tests/fixtures/engineering-telemetry/README.md)。

1. 固定 source/tested/policy、schema、输入文件摘要及运行命令，先用合成快照执行离线报告。对设计中的并行例断言反馈墙钟 21 秒、资源合计 25 秒、纯执行最长链 19 秒、长分片未归因等待 2 秒、含等待观测链 21 秒与首次启动延迟 0 秒；核对 `analysis.formula_version`、各指标的 status/value/unit/formula/input_ids 及 job_timings；改变依赖或错误地把并行时间相加时，同一断言应失败。
2. 分别删去一页、一条必要终态或依赖，注入时间逆序、未知 schema 和同身份冲突；断言 coverage、missing_reasons 及受影响指标 unavailable，不能输出完整成功。相同记录重复合并 alias 与来源列表。加入 step/phase/test 不重复增加 job 资源；条件为 false 且 skipped 可零执行，但不能成为 pass。核对 known 指针的类型和值、unknown/not_applicable 的 null 与 reason、输出无 raw payload，未知来源不能凭名称补齐。
3. 保留失败、取消与重跑的每次 attempt，核对首败不会被后续成功覆盖；错 source/tested/policy 的记录不得混合。本地输入缺测量时刻的 dirty/untracked 身份时报告不完整，事后读取当前工作区不能补成测量证据。
4. 离线核验字节上限、允许字段、截断输入与采集器失败。继续核验请求/时长/响应与解码字节额度、重复/截断分页、坏压缩流、越界/重复/链接归档、非有限 JSON、未知大字段和信号取消。截止停止新读取与记录归一化；有界保存收尾和实际 elapsed_seconds 单列，不把尾部当作预算内。只处理自有临时资源；collector 的失败单独呈现，原测试成功/失败与清理结论保持。
5. 真实接入：在一条真实 periodic-full 或 candidate-full 的 Formal 链核对 API、完整分页、实际互斥 build 分支、所有矩阵分片及 gate 其他必要上游；与原 manifest/JSONL/日志逐字段比较。旧 BASE 的 PR Formal 链仍可作为对应历史版本证据。仅得到子图时明确标 partial，不外推完整 gate 关键路径。
6. 真实接入：读取一次既有本地 compile metrics 输出，核对原 package/features、命令、commit 与环境；适配器不另启 Cargo。采集时长、文件字节和输出保留期也要记录。实际输入、报告、退出码和独立核对结论归实施 PR，不把合成例、本地结果或单次运行当作 hosted P95、缓存净收益或合入卡点证明。

有界采集命令（输出目录须已存在，partial 返回 1 是预期结果，不代表原测试失败）：

```sh
python3 scripts/engineering_telemetry.py collect-github nil-studio/oasis7 RUN_ID ATTEMPT /tmp/snapshot.json /tmp/collection.json --build-artifact ARTIFACT_ID
python3 scripts/engineering_telemetry.py collect-metrics /path/to/existing/current.metrics.json /tmp/local-snapshot.json /tmp/local-collection.json
python3 scripts/engineering_telemetry.py report /tmp/snapshot.json /tmp/report.json /tmp/report.md
```

只使用已有权限；API 请求不自动登录。可用 `--max-requests`、`--max-seconds`、`--max-bytes` 调低额度，以已知成功/失败结果检查耗尽后的局部证据。`--recorded-responses DIRECTORY` 用固定 API 原件及 ZIP 回放同一路径，文件名和摘要合同见[设计](doc/testing/ci/ci-tiered-execution.design.md#engineering-telemetry-design)。receipt.transport 区分 live/recorded/local；检查 raw_sha256、投影 content_sha256、snapshot_sha256，记录回放和真实网络成本的区别。collector 不解包执行脚本，不读取当前 Git/dirty 环境补签历史测量。

本轮真实来源样本为 [PR #5582 的 run 42371045878 attempt 1](https://github.com/nil-studio/oasis7/actions/runs/42371045878)：51/51 jobs、20/20 artifacts，实际 Formal 五分片成功，source `4dab41ef854cd521387b9980d0789ebef56d0367`、tested `2d56f4e270e0753e566ee4bb0b8d187e5cf59c91`、policy `8bb642d22d641fc9beccee78f0a032b2e3c9eeb7`。selection 与 endpoint manifest.execution_plan 一致；artifact 13574395298 的 SHA-256 为 `19daaf7cd8148c754e85a9d6461eccea319ce91726ef1da10140fbcbe4061469`，经 ZIP→gzip/tar 只读 manifest。回放按 5 次逻辑读取、7,212,629 响应字节、28,892,776 解码字节核对，不能作为 live 请求成本。job 的 tested/policy/config、数字 needs/条件、producer/consumer job 尚无正式绑定，因此真实整图指标 unavailable。原件按原服务保留期可能过期，摘要不能代替原件。已有真实本地 warm 原件尚未取得，本地真实验收未完成，不新跑 Cargo 补证；#5391 保持开放。

前向 live 入口为 `python3 scripts/engineering_telemetry.py bindings collect-live nil-studio/oasis7 RUN_ID ATTEMPT REPORT.json`，复用有界只读 GET、取消与请求/字节/时间上限，仅读取含专用观察步骤的 job 日志与实际 workflow SHA 的固定文件，不登录或触发构建。

前向 job 观察的固定导出包含 `run.json`、完整的 `jobs-N.json`、`job-ID.log` 及从实际 `workflow_sha` 取得的 `workflow-SHA.yml`。运行 `python3 scripts/engineering_telemetry.py bindings EXPORT_DIRECTORY REPORT.json --workflow-sha SHA`；输出必须在输入目录外。新 bindings/v1 报告绑定数字 job 与运行时 job key/matrix，保留观察时 checkout 与 run 目标不同的事实；Formal gate 没有 checkout，不补造 SHA。它仅证明实际取得的内容绑定，当前未核实矩阵总数、artifact ID/传输与消费者验证时返回 partial，不输出完整 DAG 或替代 v2 报告。用 `--scope required-gate` 时其他必要上游没有观察，同样必须列缺口。

回归应覆盖错 attempt、重复实例/标记、专用步骤以外的标记、来源 workflow 不符、文件符号链接、无 checkout 控制节点、两端 manifest 摘要相同但 producer attempt 不同；保留原 job 结论。观察步骤仅在非取消终态尝试，`continue-on-error` 仅适用该精确诊断步骤，不能吞掉构建、测试、上传 required 工件或 gate 的失败。新运行验收与缺口写入对应 PR，不把旧历史 run 补签为完整。

### 当前 CI 未直接覆盖（需手册补齐）
- Web UI agent-browser 闭环（现为手动/agent 流程，不在 CI 默认路径中）。
- `m4/m5` builtin wasm hash 校验（`scripts/ci-tests.sh` 已移除 `sync-m4/m5 --check`）。
- runtime builtin wasm bootstrap / default-module / body-action 闭环（已从 required 下放到 `test_tier_full`）。
- Viewer headed / GPU 真环境性能在被选择时必须采集，但在稳定可复现的环境特定采样阈值、原始复现与有时限 waiver 生命周期建立前仍是 report/watch，不是 blocking required gate。

结论：
- `commit` 是显式本地诊断，覆盖当前治理/support/viewer 清单，不由普通提交触发；它不包含 Rust fmt，也不承担 `oasis7 --tests` required shard；
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

<a id="regression-oracle"></a>
### 回归用例：公共行为与独立预期值

用例先写清触发输入/状态、真实观察点和合同预期，再选择能稳定检出缺陷的最小接缝。优先经受影响的公共入口走到消费者结果，而不是对私有函数调用次数或内部对象形状做镜像断言；一次只覆盖一个清楚行为，先观察因原症状失败，再修复并验证受影响边界。

- 预期值来自适用合同中的明确样例、独立手算或可核对的权威原件。不要调用被测 decoder、projection 或关联函数生成 expected，再与自身比较。
- 最小纵向闭环要经过本次真正要验证的接线、状态持久化或授权边界。mock 可以隔离不可控外部依赖，但不能替代正在验证的边界；只有 fixture/mock 的结果不宣称真实服务、网络或 Runtime 已通过。
- 按受影响行为复用或补充能区分正确与错误实现的正/负例。例如 cognition committed、业务 receipt rejected 时，断言候选已提交与业务已拒绝、未成功分别成立；仅断言 HTTP 200 或 committed 应不足以判成功。对原 world/request/适用 fence 做不匹配变体，核对相关校验拒绝及合同规定的无副作用结果。
- 行为稳定时，不让测试绑定无关内部拆分；重构后应继续由同一公共行为断言保护。已有用例能检出缺陷就复用，不为方法补一套同义测试。

<a id="shared-interface-conformance"></a>
### 接口共用契约验证：ModelProvider 首个试点

Trace refs: O7-REQ-f1388c2b-6f64-4ebd-886e-0b741bfe176c

本节落实[接口前置交付](doc/engineering/workflow/source-of-truth.md#interface-prerequisite-delivery)的验证方法。试点选用现有 `oasis7_agent_api::ModelProvider` 的 `CallerSnapshot` 模式；接口、DTO 和依赖权威仍为 [GameClient / Harness 设计](doc/world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001)。`ProviderConversation` 的会话历史、工具结果顺序和恢复由其原设计及专门测试负责，不因这一试点取得覆盖。

**当前能力与待实施增量。** `crates/oasis7_agent_api/src/model_provider.rs` 已唯一声明 trait、请求/响应、错误及 `complete_checked`；Harness 的 `native_task_tests.rs` 已有本地 `FakeProvider`；`crates/oasis7_model_provider/tests/transport.rs` 已通过本地 HTTP/SSE 夹具驱动真实 `OpenAiChatCompletionClient`，验证工具名、参数、使用量和请求线格式。它们目前没有共用同一套可复用行为断言，也没有 API crate 的 `test-support` feature；下述共享支撑和接入属于待实施方案，不是现成命令或已通过结果。

**支撑代码与单一写入者。** 前置实现复用现有 trait，不另造 port 或镜像 DTO。在 `oasis7_agent_api` 内增加显式启用的 `test-support` 模块，提供脚本化 fake、场景数据和公共断言；仅测试的 dev-dependency 启用，检查正常/build 依赖闭包不因此引入 Harness、真实 provider 或 Runtime。API 共享写入者统一修改其 `Cargo.toml`、模块导出和支撑代码；适配器执行者只负责 `oasis7_model_provider/tests/` 的真实构造器与 HTTP 探针；消费执行者负责 Harness 的测试接入。任务负责人把这些范围分配给唯一现任执行者并承担组合验收，可以兼任，沿用现有 Issue/PR 的交接记录。

fake 的控制面限于本例所需的每次响应/错误队列及收到的完整请求快照；每例重新构造，脚本耗尽显式失败，不自动返回成功或吞掉错误。共用断言接受实现构造器与观察探针，以相同请求和独立预期值经 `ModelProvider::complete_checked` 调用；fake 探针记录 `complete` 输入，真实适配器探针读取本地服务收到的请求。两种构造器的编码不同可以接受，但公共响应及错误判据不能复制成两份。最小消费者复用 Harness 已有的 `NativeTaskRunner` 用例，依赖同一 API 类型和 fake；生产代码的世界授权与提交路径不由测试支撑接管。

| 共用场景 | 输入/观察点 | 两种实现共用的判据 |
| --- | --- | --- |
| 合法单次工具返回 | 固定合法 digest、`quote_factory` schema；脚本返回参数 `{"factory":"f1"}` 与 total tokens `5` | 返回相同 request digest、精确工具名/参数及 usage；观察输入保持所选 prompt/tool 内容。`provider_id`、时延等实现特有信息不要求相等 |
| 请求在调用前拒绝 | 非规范 digest；另选空 model 与空 tool name；记录 fake 调用和本地 HTTP 请求数 | 分别返回 `invalid_request_digest`、`invalid_model`、`invalid_tool_schema`，`retryable=false`，且实际 provider 调用数为零。只证明模型调用未发生，不外推世界副作用 |
| 供应商暂不可用 | fake 返回 `provider_unavailable`；真实适配器收到本地 HTTP 503，使用已有 native 单次预算配置 | 保持 `provider_unavailable` 与 `retryable=true`；不把错误变成成功或假定该标志已授权额外物理调用。真实请求次数仍由适配器预算用例验证 |
| 错配/空响应防线 | 测试专用包装器在 `complete` 后、`complete_checked` 校验前替换 digest 或清空 turns，分别包裹两种实现 | 分别拒绝为 `response_identity_mismatch` 或 `empty_model_response`，且 `retryable=false`。这是公共校验防线测试；真实适配器当前从请求复制 digest，HTTP 夹具不能独立制造该错配 |

`complete_checked` 当前只做请求校验、`complete` 和响应校验，不会自动调用 `preflight`、`check_execution`，也不会校验所有预算或工具权限。非法 caller schema、未声明工具返回、native 零预算、物理重试和重定向继续由真实适配器既有测试负责；不得把这些实现专属规则误写成全部 `ModelProvider` 的默认行为。`ModelProviderError.code` 仍是现有可扩展字符串合同，本试点只断言所选稳定代码与 `retryable`，不新增全局错误枚举迁移。

**防止测试自证。** 前置 PR 中先在 fake 上运行合法场景并通过，再用测试专用包装器把返回参数 `factory` 改成另一个合法字符串，重新运行同一合法场景断言，要求失败；后继 PR 接入真实适配器后，也在真实适配器上运行合法场景及同一破坏性反例，并在实际组合上验收；不要为被破坏实现另写一个期待失败的替代业务断言。实现时可用捕获断言失败的定向测试保留此证据，或在临时变体运行后撤销破坏，不能提交永久失败的普通测试。此反例应证明原公共断言能区分行为偏差，不能只证明测试进程启动过。

**小 PR 与组合验收。** 第一个前置 PR 交付共享支撑、fake 通过及上述反例、一个实际 Harness 消费用例；API 已存在，生产行为无需等待未来实现才成立。其固定提交可供真实适配器接入 PR 消费，合入后核对实际 merge 版本；共用代码继续由同一写入者维护。后继 PR 把同一场景接到本地 HTTP 的真实适配器，并由整体负责人在实际组合上执行两种实现与最小消费者回归。若共享支撑与首次接入无法独立正确，可合成一个小而完整 PR，不机械拆分。文档交付后仍保留实现、真实接入及最终组合验收清单。

以下是已有测试入口，用于实施前盘点及实施后的受影响回归；新增共用场景落在哪个测试目标，由实现 PR 写明实际命令与受测版本：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_agent_api model_provider::tests
env -u RUSTC_WRAPPER cargo test -p oasis7_model_provider --test transport --test physical_budget
env -u RUSTC_WRAPPER cargo test -p oasis7_agent_harness native_task::tests
```

本地 HTTP 通过只证明生产适配器在受控传输中的映射行为；不需要外部模型密钥，也不证明供应商可用性、真实认证、费用、持久化、Stop/撤权或 Runtime 提交正确。重复调用在此接口上不承诺去重；请求身份不等于 exactly-once。需要上述能力的任务沿原设计补充真实边界验收，不因 fake、共用断言或文档合入而关闭。

<a id="request-recovery-regression"></a>
### 原请求与恢复路径的定向回归

只选择本次修改涉及的场景，并先确认当前实现与适用合同。API 身份、幂等作用域、错误码及保留边界以 [Game API 设计](doc/world-simulator/llm/external-agent-game-api.design.md)和[外部 Runtime 验证手册](doc/testing/manual/external-agent-runtime-contract-validation.manual.md)为准；下表是用例选型，不新增接口、统一 TTL、数据库方案或 exactly-once 承诺。尚未实现的路径保持 planned/未覆盖。

| 受影响场景 | 输入或故障位置 | 观察与判据 |
| --- | --- | --- |
| 同意图重复 / 并发 | 在合同允许重投的入口，保留完整原身份与原 payload 重投或并发到达 | 返回原结果或明确未决，权威业务效果不重复；不能只数响应次数 |
| 同标识异内容 | 在合同定义的同身份范围改变 payload；另保留不同合法意图作正例 | 冲突/拒绝而非误回另一意图的成功；合法不同意图不被误合并 |
| 效果后、响应前断连或崩溃 | 原请求已发出，调用方缺最终响应，再恢复 | 先查原请求；仅在该合同已确认允许安全重投时保留原签名/nonce/payload/correlation 重投，否则只回查并报告阻塞；不换 ID、world 或 Context 绕过未决 |
| pending / unknown / 存储暂不可读 | 返回未决、无法读回或原证据不足 | 保持未确认并遵循该合同的新写限制；不把无响应/读取失败当业务失败，不制造竞争写 |
| 到期、Stop 或撤权 | 分别尝试原结果回读与新副作用 | 读取仍检查当前读授权；旧 grant/fence 不因恢复复活，新写按当前合同拒绝或停止 |
| recent window / 历史保留边界 | 结果退出近期窗口，或原历史材料不可得 | 窗口缺项不证明失败；沿支持路径查原结果或当前资源，区分 HistoryUnavailable 与 Unknown，不用新请求填补失去的历史 |

恢复验证应分别回答“旧请求有没有重复”“原结果能否读回”“恢复后能否继续服务”。若本次结论包含恢复后可服务，按适用的 [V-RECOVERY / J1、J2 正向继续要求](doc/testing/prd.md#qa-infrastructure-scene-families)选择实际层级：在原未决已按合同处理、当前 authority/fence/权限/资源及预算允许后执行获准的新意图，核对权威效果与后续推进；不能用只读恢复、root 相等或效果至多一次代替正向结果。不相关的接口/UI 修改不因此扩成完整基础设施恢复验收。

判定记录沿用实际 PR/Task 与所选套件的原证据，标清执行/未执行和环境边界。故障注入只在授权隔离环境进行；固定 fixture 验证不代签真实 Runtime、浏览器或部署恢复。

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

<a id="testing-layer-model"></a>
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

<a id="playability-l5"></a>
### L5 真实人类 / 受控线上验证层

- 目标：验证真实人类或受控外部玩家在真实时间、注意力和机会成本约束下，是否仍愿意继续玩；这是 `L4B` 之上的正式验证层。
- 入口：limited preview、liveops 反馈回流、真实玩家 session、受控人工试玩样本。
- 受控首局操作：按[首局真人试玩手册](doc/testing/manual/first-session-human-playtest.manual.md)准备候选、筛选目标玩家并主持两次会话，保留独立完成、提示、停止和回访的原始记录；预约回访与主观意愿不推导自然留存。
- 结论边界：只有到这一层，才能正式回答“真实人类/真实环境里是否仍想继续玩”；内部真人 spot-check 仍只算 `L4B` 的可选校准，不单列新层。
- 说明：长稳、压力和 soak 仍属于测试套件层（如 S8/S10），不是这里的 playability `L5`。

<a id="playability-review-persona-and-claim"></a>
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
# 真实增量 Clippy：源码 lint 与 clippy.toml 变化都必须重新生效
./scripts/cargo-dev-sccache.test.sh --real-clippy
./scripts/check-rust-file-size.sh
env -u RUSTC_WRAPPER cargo fmt --all -- --check
```
- S0 是适用于任何改动的快速静态基线：不运行 Viewer/Bevy、wasm 或任何其他 target-specific 编译；这类验证必须按改动面进入 scoped required（例如 S5）。
- 本地日常迭代若只是为了更快得到开发反馈，默认把同类命令替换为 `./scripts/cargo-dev.sh check/test/run/build ...`；各 worktree 默认使用普通本地 `target/`，wrapper 不改变正式验收入口。但进入正式 required/full 验收、尤其是 deterministic wasm/release 相关链路时，仍以本手册列出的原始 cargo 命令为准。
- 本地脚本内的开发态 build/test/run 应通过 `scripts/cargo-dev-lib.sh` 使用本地 target 与开发 profile；正式验收脚本、release workflow、deterministic wasm、module release acceptance、hash/receipt evidence 仍不得被该 helper 隐式改写。
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
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required tests:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --lib --features test_tier_required tests:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7 --lib --features test_tier_required simulator::tests:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --features test_tier_required viewer::live::tests:: -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --features test_tier_required web_bridge::tests:: -- --nocapture
```
- 电价/市场机制定向回归（required/full）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --lib --features test_tier_required tests::power::power_buy_zero_price_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --lib --features test_tier_required tests::power::power_order_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --lib --features test_tier_full tests::power:: -- --nocapture
```
- 主链 Token / NodePoints 桥接定向回归（required/full）：
```bash
./scripts/main-token-regression.sh required
./scripts/main-token-regression.sh full
```
- 修改定向脚本时，PR 的 `workflow-governance` 选中候选脚本的轻量合同测试，确认选择真实 Runtime 包与筛选路径，并拒绝零用例或 Cargo 失败；普通 Rust 分片的用例通过不代替这个入口测试。自然与手动候选 full 共用 `full-extended`，直接调用 `main-token-regression.sh full`，要求五次实际选择都非空且通过；无关 PR 不额外运行这五次 Cargo 命令。
- 当前运行与审计权威：`doc/p2p/token/mainchain-token-allocation-mechanism.prd.md`。
- 用途：
  - 按实际测试归属快速定位 Runtime、Simulator 或 Viewer 回归；`oasis7` 命令保留应用层 Simulator 适配测试，不能代替 `oasis7_simulator` 的内核用例。不必每次跑全套 full。定向验收应核对选中的用例及实际执行数；零匹配不算通过。

<a id="s4b：governance-registry-drill（l4-前置-l4）"></a>
<a id="governed-validator-admission-authentication-controlled-profile-v2"></a>
<a id="动态集合-noderuntime-bft-v2-工程路径"></a>
<a id="本地-cas-retention-与-gc-协调"></a>
### S4：分布式子系统套件（L3）

S4 分布式子系统套件正文已按 #6507 拆分至分册 [`doc/testing/manual/testing-manual-s4-distributed-and-governance-drills.manual.md`](doc/testing/manual/testing-manual-s4-distributed-and-governance-drills.manual.md)，不改变既有命令与验收边界。

### S4B：Governance Registry Drill（分册入口）

S4B Governance Registry Drill 正文已按 #6507 拆分至分册 [`doc/testing/manual/testing-manual-s4-distributed-and-governance-drills.manual.md`](doc/testing/manual/testing-manual-s4-distributed-and-governance-drills.manual.md)，不改变既有命令与验收边界。

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
- 受控浏览器功能回归使用显式 `fixture-functional` profile，用真实组件/状态处理复现响应时序；统一 runner 已覆盖 BFR-P01–P07 并接入 `viewer_js_required` 选测，详见[受控浏览器合同](doc/testing/manual/web-ui-playwright-closure-manual.design.md#browser-functional-fixture-contract)与下方[实施核验](#browser-functional-regression-validation)。其合成依赖结论与真实栈/正式 S6 分开记录。
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

<a id="browser-functional-regression-validation"></a>
#### 受控浏览器功能回归核验（实施阶段）

Trace refs: O7-REQ-d7eec6c8-06c7-4707-8f50-81b19e910c78；对应 [AC-14C / BROWSER-AC-1–5](doc/testing/prd.md#browser-functional-regression)。现有 `npm --prefix crates/oasis7_viewer run test:ui` 是 Vitest/jsdom；`./scripts/viewer-software-safe-step-regression-smoke.sh` 是 tooling fixture 预检；`node crates/oasis7_viewer/scripts/player-visual-feedback-browser-smoke.mjs` 已使用真实 shell 和固定 bridge/feed 采证。三者各保留原证明范围，均不能单凭入口存在宣称[全部 BFR-P01–P07](doc/testing/manual/web-ui-playwright-closure-manual.design.md#browser-functional-pilot)已覆盖。统一入口为 `crates/oasis7_viewer/scripts/browser-functional-regression.mjs`（CI 组 `viewer_js_required`）；`--cases`/`--viewports` 是同一矩阵的精确选测，空选择、未知 ID、重复项、零执行、缺产物和自检失败均非零退出。

```bash
npm --prefix crates/oasis7_viewer run build:viewer:visual-test
npm --prefix crates/oasis7_viewer run build:viewer
node crates/oasis7_viewer/scripts/browser-functional-regression.mjs --skip-build
```


1. 在实施候选记录准确 source/tree、依赖锁定和测试构建身份，先运行适用 JS-required；确认服务的 bundle 来自当前真实 `viewer_app.jsx` 与 PromptControl/feedback 模块。核对 test entry、synthetic auth/bridge/feed/transport 的替换清单，release 加 query 不能开启控制器。只为本 profile 启动本地静态资源和受控依赖，不启动完整 worktree launcher/active LLM 栈。
2. 为每 case 新建独立命名 context、loopback 端口、storage 和产物目录，先用一个非 allowlist HTTP/WebSocket/worker 请求确认网络在发送前被阻断；核对没有继承用户 session 或真实 provider 配置。共享构建先串行冻结，case 并行前检查实际隔离；失败/取消也只回收自己拥有的资源。
3. 按 BFR-P01–P07 的输入/预期执行真实可见控件，先等出站请求和 pending 条件，再按 barrier 释放消息/时钟。至少保存 desktop 与 narrow 的声明视口结果；P04 在 B 的 signingEntered 后交付 A，再 releaseSigning 并确认 B 实际发送，另案覆盖 sent 期；记录 deferred signer 只延迟真实签名结果的替换边界与 P06 连接代次。控制器不得直接写期望 core state，`__AW_TEST__` 仅用于读回；用固定次序重放失败，不能自动重试取绿。
4. 给每个已选 case 核对请求次数、关联字段、当前草稿/授权版本、反馈状态及安全下一步，保存实际截图、trace、console/network、准确未满足断言与清理结果。拒绝场景不提前显示 applied；只清理属于原等待的 pending，不能清新请求 timer。若出现生产缺陷，保留红色结果并路由原领域 Issue，不修改 fixture 使其通过。
5. 分别验证缺浏览器/产物、空选择、未识别请求、超时/取消、日志写失败和跨 case 状态污染；必要 case 不能 skipped 后汇总为 pass。后续 CI 接线用受影响 Viewer/state/transport/测试装配输入的正例与无关文档改动反例核对同一 planner，选择与执行不一致直接失败。首片只运行 P01–P03 时如实报告剩余矩阵。
6. 将产物与结果留在实施 PR/CI。fixture pass 只证明所测真实前端在合成输入下的行为；真实认证、绑定恢复、Runtime 应用、provider/world delta 仍按原 S6/PWT 及领域验收补证，不能换算成 release、跨浏览器或 L4B/L5 通过。本次文档 PR 未执行上述浏览器核验。

### S7：场景矩阵回归套件（L1 + L4）
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --features test_tier_required scenario_specs_match_ids -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --features test_tier_required scenarios_are_stable -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_simulator --features test_tier_full oasis7_init_demo_runs_ -- --nocapture
```
- 配套文档：`doc/world-simulator/scenario/scenario-files.prd.md` 的“场景测试覆盖矩阵”。

### S6.5：Chain Runtime Storage Profile / Gate 技术核验
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib node_runtime_execution_driver_uses_storage_profile_checkpoint_interval -- --nocapture
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
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required tests::persistence::recovery_tests::rollback_with_reconciliation_recovers_from_detected_tick_consensus_drift -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --lib --features test_tier_required runtime_authoritative_recovery_rollback -- --nocapture
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
- 同等覆盖的手工命令（省略脚本重复的 Net 原生执行）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7 --tests --features test_tier_required
env -u RUSTC_WRAPPER cargo test -p oasis7_node
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib
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
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required tests::governance::governance_identity_penalty_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required governance_identity_penalty_and_appeal_drive_vote_rights -- --nocapture
```
- 通过标准：
  - 同目标主体 + 同证据哈希的惩罚重放被拒绝（incident 指纹不重复通过）。
  - 惩罚 -> 申诉 -> 复核后 `evidence_chain_hash` 逐阶段变化且 `appeal_evidence_hash/resolution_evidence_hash` 非空。
  - `governance_identity_penalty_monitor_stats` 输出误伤率与高风险未闭环数量。
- 经济源汇审计与阈值门禁（TASK-GAME-016）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required main_token_economy_ -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required main_token_treasury_distribution_applies_closed_loop_and_records_audit -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required main_token_fee_settlement_burns_supply_and_tracks_treasury_buckets -- --nocapture
```
- 通过标准：
  - 审计报表输出 `mint_total/burn_total/net_flow` 与当期 `issued/distributed` 指标。
  - `enforce_main_token_economy_gate` 在 `inflation:*` 或 `arbitrage:*` 告警时返回阻断错误。
  - 报表中 `exploit_signature` 可用于治理升级与 runbook 分诊。
- 可运维发布阻断门禁（TASK-GAME-017）：
```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --features test_tier_required longrun_operability_release_gate_ -- --nocapture
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

<a id="release-candidate-reception-validation"></a>
#### 同候选整包接收核验（RC01–RC10）

本方法接收 [QA 候选整包设计](doc/testing/design.md#release-candidate-evidence-reception)，对应 [REQ-ENTRY-CANDIDATE-001 / AC-ENTRY-CANDIDATE-001](doc/product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)；Trace refs: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b。上面的 release 脚本仍只执行其声明的工程 steps；命令 exit 0、bundle hash 校验、fixture Web 或 dry-run 均不自动执行本方法，也不能补足经济、恢复、实际发行材料或另一 mode。

**准备与层级。** 先在[原发布证据模板](doc/testing/templates/release-evidence-bundle-template.md)固定候选、实际 tested tree/build/配置、待评阶段/claim、两 primary mode 与受支持的平台/入口、各原合同及 required/full 场景。按设计 §7.1 的输入表逐项关联 owner、实际窗口/单位、world/原操作/authority 与原始 artifact。所需输入的适用性由原产品/专业要求确定，不以手头有什么日志决定。使用[既有 source-to-scene 方法](doc/testing/prd.md#qa-infrastructure-source-scene-method)分别核身份、字节完整性、真实执行、领域 oracle、环境与消费者。

文档设计走查可使用明确标为示意的 A/B 候选及输入副本核下表判定，不启动世界或模型、不改生产记录；只能记录“设计预期一致/不一致”，不能填写运行 pass。实际候选验收使用本候选按原方法取得的真实结果：Viewer 与正式 API 的[逐入口方法](doc/testing/manual/external-agent-runtime-contract-validation.manual.md#api-primary-entry-validation)、Runtime/P2P 的原证明与恢复组合、[经济 EA01–12](doc/testing/manual/mature-world-economic-audit-acceptance.manual.md#economic-audit-01)、发行/安装与[材料方法](doc/readme/governance/player-support-and-service-lifecycle.manual.md#verify-release-materials)分别承担原测试层级和环境。静态、受控 fixture、正式运行、实际生产与真人证据分别记录；不得用本接收方法新增一份运行证明或降低原必跑项。

| 方法 | 输入与操作 | 独立预期与原件保留 |
| --- | --- | --- |
| RC01 完整正例 | 固定候选 A 的原范围：两 mode 及每个声明支持的平台/入口；取得各自真实运行身份/消费者结果，同 world 与合法 authority 的执行和正常恢复、该阶段原要求的完整经济窗口、实际资产/适用材料、QA 各层结果及范围一致的公开草稿。逐行回读原件和独立 oracle，再把跨域切点/窗口接上 | 所有必需项及对应关系均满足时，QA 的该范围结论为 `pass`，可交原产品决定；各 mode 与专业判据保留自己的结果，不生成发行 verdict。后续发布是否评估、批准或执行分别保留真实记录 |
| RC02 混候选或错身份 | 在 RC01 的评审输入副本中分别换入 B 的 tree/build/配置、另一 world/branch 的 receipt、旧资产 digest；保留同 tag/文件名作为混淆。另以正确标明的旧版本升级前态作对照 | 身份不匹配的输入不能成为 A 的通过证据；对应必需项缺有效材料则 `blocked`，已观察到的实际错配仍保留原 `fail`。合法旧前态仅作因果输入，须由 A 的实际恢复结果衔接，不能使用 B 的 pass 填 A |
| RC03 仅一 mode 或互相代签 | 从两 mode 声明中去掉 pure_api 或 viewer 的实际消费者证据；用另一端截图、同一 receipt 的副本、alias 或 provider 名称补位；另保留单 mode 专题范围对照 | 整包的双 mode 对应保持 `blocked`；已有单 mode 只能支持自己的实际窄结论。共享 canonical 原件可以复用，入口的动作/读取/恢复证据仍各自成立，不要求为了复制证据重复世界效果 |
| RC04 窗口与 authority 不对应 | 分别混入相同 tick 字符串但不同 root/branch、缺合法 proof 的执行记录、经济 C0/C1 与消费报告不符、规则/manifest 变化却未保留对应证明的材料；与同一不可变原件的合法异步回读比较 | 不因日期、tick、状态截图或 schema 相同而接收；不明/缺失关系 `blocked`，违反实际 oracle 保留 `fail`。合法异步回读必须指回原窗口与适用 authority，不能缩短长期审计窗口或发明容差 |
| RC05 正常恢复结果仍未知 | 从必需正常恢复输入去掉原权威结果，只保留已正确显示 unknown/recovery_required 的反例；另给原请求曾提交但回应丢失、随后在当前读权下查回原结果的正例 | 正确显示未知可通过自身失败处理断言，但不补正常恢复的缺证，整包该项仍 `blocked`。正例必须回到原身份和原结果，按原合同核一次效果/无重复结算；不能以新请求、换身份或查不到结果认定未发生 |
| RC06 经济不完整或存在异常 | 分别替换为四 tick/部分资源样例、缺页或未知 writer 的报告、旧候选空告警；另放入当前完整原件可独立确认的经济异常，再以按原完整窗口核算通过的报告作对照 | 组件算对或无告警不补阶段要求的长期/全范围审计；缺完整来源/范围则 `blocked`，确定违反原经济判据则保留 `fail`。完整正常报告才支持该经济输入，仍不单独放行整包；不新增补偿、阈值或货币规则 |
| RC07 发行资产、权利或入口链缺证 | 分别缺少声明平台资产、完整性/来源资料、适用权利或渠道材料；另令网页候选正确但实际加载旧 build、错误 backend/mode，或适用升级/恢复证据不足 | 每个受影响组合保持缺证或原实际失败，不能用另一平台、下载 manifest、占位素材或已批准文案补足。未选择的新渠道仍按原适用性，不能为凑材料新增开放承诺 |
| RC08 QA 到公开状态交接 | 从 RC01 的完整输入形成 QA 有界结论，核其实际产品决定、LiveOps 草稿与根 README；分别加入尚未评估发布、仅审核草稿、扩大 mode/平台/能力或使用旧 QA 结论的变体 | 未评估发布按原模板记 `not_evaluated`；reviewed/publish_ready 不写 published。只有原决定、同窗证据和根 README 支持的事实才可进入原发布流程；公开状态仍以真实发布/同步记录为准。本方法不执行外发 |
| RC09 环境或层级替代 | 对声明必须真实运行/正式入口的项，分别换成 mock provider、fixture-functional、GET-only observer、缺必需 full 场景或 skip/dry-run；保留这些样本在自身原范围内的正确结果 | 受控结果继续有效于原 scope，但不能代签正式能力、parity、恢复或更高层。缺必需环境/执行则 `blocked`；原实际失败不得被低层 green 遮蔽，选中层级仍沿原合同 |
| RC10 变化、补证与重试 | 在已评输入后改变实际 build/规则/配置/authority/材料或受支持范围，核受影响输入及消费者；追加修复后的新 attempt，再比较旧失败、补齐材料与未受影响来源 | 新身份与受影响判据重新关联，不能只改候选昵称搬用 pass。旧失败和各次 attempt 保留；只有修复后的独立结果确实解除原阻断，才按原 QA/产品职责更新当前结论。未受影响材料的原身份/范围仍成立才可复用 |

**接收步骤与输出。** 按 RC01 建立可通过的完整对应，再逐一核 RC02–RC10，不能只验证错误输入被拒绝而没有正常路径。将结果填回模板原有“实际覆盖与未证明边界”和“结论摘要”：每项原输入/方法、实际执行层级、oracle、verdict、对应关系、原 artifact 的可回读定位/完整性、受影响范围与 owner 都可检查；已执行失败、未执行、缺前置和原不适用依据分开。缺证时保留 affected gate 与下一步，禁止先填总 pass 再附注例外。

QA 只形成原声明范围的证据结论，producer 负责原阶段/claim 决定，LiveOps 按该决定和 QA 限制使用[既有沟通模板](doc/readme/governance/readme-release-communication-template.md)；当前公开状态最终核根 README。需要收窄时保留原整包未满足项，不将更窄结论写成原候选门禁通过。本次方法补充未执行上述真实候选、素材制作、真人或发布动作。

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
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib module_release_submit_attestation_ --features test_tier_required -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib module_release_apply_rejects_when_attestation_threshold_not_met --features test_tier_required -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib module_release_apply_rejects_when_attestation_receipt_evidence_mismatches --features test_tier_required -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib power_bootstrap_release_manifest_full --features test_tier_full,wasmtime -- --nocapture
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
  - 默认输出目录：`.tmp/world_runtime_finality_baseline/<timestamp>-<unique>/`
  - 归档文件：`summary.md`（人读）与 `summary.json`（机器读）
  - 每个 case 固定 `oasis7_runtime --lib`、required/full feature 和 `tests::governance::…` 完整 identity，以 `--exact --format pretty --color never` 运行。每次 warmup/sample 均校验退出码、唯一目标 `ok` 与非零通过计数；零命中、ignored、错目标或未知输出均失败，失败样本不计入耗时。
  - 原始 `logs/<case-id>/*.log` 及对应 `*.result.json` 保留逐次退出码、identity、观测计数和校验结果；`summary.json` 的 case 新增 `test_package/test_target/test_features/test_identity/executions`，原有字段与 `cases.tsv` 格式保留。dry-run 仍有 `dry_run=true`/`note=dry_run` 且无执行记录，零样本与 skip-full 明确 skipped。
  - 脚本回归：`python3 scripts/oasis7-runtime-finality-baseline.test.py`。计时仍覆盖整条 Cargo 命令，不能作为热缓存单证书性能或全节点传播收敛验收。
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

<a id="test-issue-acceptance"></a>
## 测试 Issue 接收与验收

本节是 `type:test` 执行者的操作入口，适用于开发 PR 合入后的剩余复杂测试和独立测试任务。职责、认领、转换与关闭条件由[测试任务的接收与完成](doc/engineering/workflow/source-of-truth.md#test-task-acceptance)定义；Trace refs: O7-REQ-1a003dc5-18f2-4cd3-adbb-d1b95316086e。测试负责人可以是人或 Agent，复用原 Issue、现有测试方法和证据，不另建测试台账。

开发角色随 PR 完成本项单元测试及不依赖外部条件、可自行准备运行的集成测试，保留已有合入前必要验证。无剩余复杂测试时，合入并满足全部原交付义务后直接结项；需要部署、真实外部系统、专门资源或持续观测等条件的剩余复杂测试，按主规范在所需开发 PR 均合入后沿用原 Issue 转入，再执行下述步骤。

### 1. 读取任务并确认接收

从用户指定、已有负责人交接或获准范围内的开放测试 Issue 接收任务。读取当前正文、必要评论、父项验收及关联 PR，区分本项原目标是合入后的复杂测试还是独立测试，保留全部未完义务。代码转入时核对开发测试证据、本项约定实现所需 PR 的实际合入记录及剩余复杂测试；开发未交付时保留代码任务和原执行者。独立测试按其约定对象与版本接收。核对实际 source/build、部署版本、环境、验证入口、权限/预算和退出条件；尚待部署或其他外部条件时，列明可先测的范围及仍需等待的条件。

检查已有测试认领、会话、暂停和在途操作，再按主规范在原 Issue 确认本轮测试范围、执行者、固定版本及环境，并回读。原开发会话和本轮测试范围分别记录；发现重复认领或写入结果未知时，先核对已有结果。缺环境或接收能力时说明阻塞和解除动作，原负责人继续跟进；仅被列为下一责任人不能报告测试已启动。

### 2. 对账证据与准备缺项

对照原验收逐项读取实际证据，说明哪些已证明、哪些待验证、哪些受阻；已有等价记录直接引用。核对证据的版本、环境、输入、预期和实际覆盖，PR 自述、单个绿色检查或旧截图不能补足未验证范围。仅在原条件仍适用时复用已有结果；证据已齐时直接进入结论核对。

按本手册的测试选择入口和相关领域方法确定剩余套件、场景及独立预期。检查实际测试环境身份、必要依赖和观察点，分配不冲突的进程、端口、数据及输出位置。需要新建测试框架、持久用例或 CI 接线时，按代码任务合同明确实现承接；临时诊断和已就绪的独立测试仍可继续。涉及自主试玩或真实 UI 时，进入适用专项手册，保留其独立执行及证据要求。

### 3. 执行并记录实际结果

固定本轮候选，按下方[共用执行剧本](#test-execution-playbook)运行选定范围，记录实际命令/操作、输入、预期与结果、失败和未覆盖项。失败只阻塞依赖该结果的测试，其他已授权且可独立执行的范围继续。源码、构建或相关环境发生变化时，保留原尝试的版本与结果，核对实际影响并安排必要复测；无关 main 前进不自动触发全量重跑。

缺前置、超时、执行结果未知或证据无法读取时，报告具体原因及受影响范围，不将未执行算作通过。停止或中断时核对自有测试进程、外部操作及资源，保存已完成覆盖和恢复入口。续做或换人前先回读原认领和实际执行状态，按主规范确认停写/停止及暂停边界，不重复启动尚未确认结束的测试。

### 4. 反馈问题并接收修复

在原 Issue 记录失败 source/build、环境、最小复现、预期/实际和可读取证据，按[失败分诊](#test-failure-triage)判断问题，并沿主规范的设计检查和问题归属规则承接。代码转入验收的同源实现缺陷交回原开发会话，原项保留剩余测试；独立测试发现的问题关联有效实现项或独立缺陷，继续本项覆盖。环境缺口记录责任人与解除条件，不因纯部署等待启动代码修复。

收到修复后回读实际 PR/提交、构建及环境；代码转入的返修须确认开发者已完成相应回归且修复 PR 已实际合入，再接收新的合入候选复测原失败及受影响范围。独立测试按原定版本及复测约定核实修复已进入待测对象。记录旧失败、新候选和各自结果，不以一次重跑成功覆盖前次事实；仍有必要未测项时保持对应义务。

### 5. 交付结论与结束

复用下方[结果记录模板](#test-result-record)或等价正文，在原 Issue 说明实际覆盖、逐项验收结论、未测/受阻范围、问题承接、下一责任人及动作。测试结论分别说明本轮是否结束、原退出条件是否满足和 Issue 是否可关闭。代码转入验收继续核对原代码目标及适用合入/部署等义务；独立测试按自身退出条件结项，保留关联缺陷及约定的后续复测。

有关闭权限且全部原义务满足时，回读最新要求和状态后关闭并核实结果；缺权限则交给有权限的负责人执行。已处于测试阶段且当前有效证据完整时，可同轮完成接收、结论及获准关闭，适用的独立验证仍须有实际证据。按原资源清理规则处理本轮自有环境，未结束或仍用于排障的材料保留用途和后续动作。

<a id="test-execution-playbook"></a>
## Human/AI 共用执行剧本

### 阶段 A：确定测试范围
1. 按前部“默认测试选择树”识别实际影响与消费者。
2. 生成本次适用的套件和定向回归列表。
3. 在日志中写清“为什么跑这些、不跑哪些”。

### 阶段 B：先跑低层，后跑高层
1. 先执行 S0。
2. 再执行对应的 L1/L2/L3 套件（S1/S2/S3/S4/S5）。
3. 最后执行 UI 闭环与压力（S6/S8；分布式改动需补 S9 或 S10）。
4. 失败时停止依赖该结果的上层测试，先定位并按职责修复；其他独立就绪范围可继续。

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

<a id="test-result-record"></a>
### 结果记录模板
```md
- 原 Issue 与测试目标（合入后复杂测试 / 独立测试，适用时）：
- 开发测试证据及所需 PR 合入记录（代码转入时）：
- 目标变更：
- 触发路径：
- 执行者（Human/AI）：
- 实际 source/build、环境及原认领关联：
- 原验收与已有证据：已证明 / 本轮待测 / 受阻范围及依据
- 套件清单（S0~S10）：
  - Sx: 命令 / 结果 / 证据路径
- 失败分诊：
  - 层级（L0~L5）：
  - 原因分类（确定性/环境/flaky）：
  - 处理结论：
- 最终结论：本轮测试结果、覆盖与未测范围、原退出条件是否满足
- 遗留事项：缺陷承接、阻塞及解除条件、下一责任人和动作
- Issue 结项：可关闭 / 保持开放及原因（适用时）
```

<a id="test-failure-triage"></a>
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
<a id="qa-economic-points-eligibility"></a>
<a id="qa-economic-points-allocation"></a>
<a id="qa-economic-points-recovery"></a>
<a id="qa-economic-points-asset-gates"></a>
<a id="qa-economic-power-ledger"></a>
<a id="qa-economic-power-redemption"></a>
<a id="qa-economic-power-signature"></a>
<a id="qa-economic-power-audit"></a>
<a id="qa-economic-genesis-allocation"></a>
<a id="qa-economic-genesis-control"></a>
<a id="qa-economic-genesis-freeze"></a>
<a id="qa-economic-genesis-reward"></a>
<a id="unified-world-resource-verification"></a>
<a id="uws-v-local-spawn"></a>
<a id="uws-v-artifact"></a>
<a id="uws-v-status"></a>
<a id="uws-v-consumers"></a>
<a id="uws-v-seed-reference"></a>
<a id="uws-v-resource-budget"></a>
<a id="uws-v-observation-parity"></a>
<a id="discovery-observation-local-trust-boundary"></a>
<a id="discovery-rendezvous-trigger-provenance"></a>
<a id="discovery-record-v1-time-policy"></a>
<a id="discovery-record-refresh"></a>
<a id="traffic-durable-attempt"></a>
<a id="snapshot-durable-attempt"></a>
<a id="host-durable-attempt"></a>
<a id="host-failure-report"></a>
<a id="observability-consumer-routing"></a>
<a id="durable-observability-attempt"></a>
<a id="traffic-freshness-v2"></a>
<a id="full-sync-v2-issuer-management"></a>
<a id="full-sync-v3-provider-lease-log"></a>
<a id="full-sync-v3-provider-read-gate"></a>
<a id="discovery-tombstone-v2"></a>
<a id="discovery-receiver-floor-v2"></a>
<a id="discovery-record-admission-budget"></a>
<a id="discovery-durable-publication-v2"></a>
<a id="discovery-work-budget"></a>
## 节点既有经济合同 QA 场景与 Discovery 观测（分册）

原末尾“节点既有经济合同 QA 场景（planned，未执行）”、“统一 seed/resource 已采用子范围与正式目标验证计划”及“Discovery observations and local bootstrap trust”正文已按 #6507 拆分至分册 [`doc/testing/manual/testing-manual-planned-and-discovery.manual.md`](doc/testing/manual/testing-manual-planned-and-discovery.manual.md)，此处保留既有稳定锚点以维持上下游引用。
