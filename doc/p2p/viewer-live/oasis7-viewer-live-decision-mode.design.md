# `oasis7_viewer_live` 决策模式设计

> 对应需求: `doc/p2p/viewer-live/oasis7-viewer-live-decision-mode.prd.md`
> 对应执行台账: GitHub Issue / GitHub Project

## 结构

1. **解析层**：`CliOptions` 默认 `llm_mode=true`；`--llm` 写入 true，`--no-llm` 写入 false，因此重复 flag 遵循最后一次写入。
2. **配置层**：解析结果映射到 `ViewerLiveDecisionMode::{Llm, Script}`，仅配置 direct viewer live 服务。
3. **入口边界层**：launcher 的 gameplay boot 不复用 Script 作为成功回退；其无 LLM 负向路径应在启动前失败并给出 direct observer/debug 提示。
4. **控制面层**：viewer CLI 只承载 bind、Web bridge、决策模式和既有链状态/提交连接参数；release/node/runtime 选项被解析层拒绝，避免双控制面。

## 表现与验收边界

- `--no-llm` 的可见语义必须是 observer/debug only，而不是“降级后正式可玩”。
- 不改变现有页面、DOM、Web test API 或视觉流程；未来若更改 launcher 的无 LLM 表现、CTA 或玩家流程，须由视觉交互专业结论定义 S6 验收。
- 直接 Viewer 与 launcher/Web 的证据不能互相替代：前者的 Script 截图/日志不构成后者的 gameplay 或 release 证明。

## 代码接点

- `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs`
- `scripts/worktree-harness.sh`
- `doc/world-simulator/viewer/viewer-manual.manual.md`

本设计记录现有结构，不授权新增 fallback、CLI 选项、provider 策略或链控制能力。

## 目标与非目标

Owner role：runtime_engineer / viewer_engineer；launcher 与 QA 分别审读入口失败及证据分类。本文补充既有四层结构的可追溯关系；审读源码基线为 `5f6239df8904fd1d2d978d734095e2c8f0bca8f4`。源码/测试定义审读仅表明静态接点，不表示测试已运行、玩法已验收或部署已开放。

目标是让默认 LLM、最后 flag 胜出、Script observer/debug、launcher 禁止成功 fallback、legacy 参数拒绝可逐项定位。原结构、表现与验收边界及代码接点保持有效。不新增 provider、场景默认值、链控制面、状态机、页面或发布承诺；本次小型兼容补充不扩写无关容量、事务和部署章节。

## 上游约束与相关角色

产品关系只覆盖入口证据不得代签与失败不得伪装成功的适用部分；默认 CLI 与 legacy 解析是专业工程义务。所有条件仍以[专业 PRD](oasis7-viewer-live-decision-mode.prd.md#当前合同)、Viewer 手册及总测试手册为准。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [默认模式合同](oasis7-viewer-live-decision-mode.prd.md#vl-decision-contract) | direct viewer 无 flag / 显式 --llm 为 Llm；--no-llm 为 Script | [默认与映射](#vl-des-default) | runtime_engineer；CliOptions 与 ViewerLiveDecisionMode | 产品适用性：N/A；reason=CLI 布尔默认值和枚举映射是既有专业配置规则，本行不承接玩家入口能力承诺；scope=仅 direct viewer 参数到配置映射，证据分类另见下行 |
| [顺序规则](oasis7-viewer-live-decision-mode.prd.md#vl-last-flag) | 多个模式 flag 线性解析，最后出现者胜出 | [flag 顺序](#vl-des-order) | runtime_engineer；parse_options | 产品适用性：N/A；reason=重复 CLI 参数优先级是专业解析规则，不定义玩家恢复或模式能力；scope=仅 direct viewer 参数顺序，不将 launcher 参数视作同义 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[observer 证据](oasis7-viewer-live-decision-mode.prd.md#vl-observer-evidence) | Script 只证明 direct observer/debug；LLM 启动亦不代签 gameplay、launcher、release | [证据隔离](#vl-des-observer) | viewer_engineer / qa_engineer；Viewer 手册、S6 | primary mode 完整分类、parity、正式玩法及发布验收未由本设计覆盖 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[launcher 失败边界](oasis7-viewer-live-decision-mode.prd.md#vl-launcher-product-boundary) | launcher gameplay 要求 active LLM；无 LLM fail fast，不以 Script 成功回退 | [launcher 拒绝](#vl-des-launcher) | launcher owner；run-launcher-stack 与 worktree-harness、game launcher | 其余 provider、身份/会话、后端失败和 UI 恢复完整流程未覆盖 |
| [legacy 拒绝](oasis7-viewer-live-decision-mode.prd.md#vl-legacy-control-rejection) | 拒绝 release-config、runtime-world、node-* 等旧控制面并给出对应迁移指引 | [单一控制面](#vl-des-legacy) | runtime_engineer；P2P PRD-P2P-010/011、chain runtime owner | 产品适用性：N/A；reason=退役 CLI 控制面拒绝是既有专业兼容边界，不产生新玩家能力；scope=仅 viewer CLI 拒绝与引导，不证明链 runtime 启动/恢复 |

## 本地设计条款

<a id="vl-des-default"></a>
### 默认与配置映射

沿用[结构](#结构)第 1、2 项：CliOptions.llm_mode 默认 true，--llm/--no-llm 分别写 true/false，initialize_viewer_server 将结果映射为 Llm/Script。该布尔选择不授予链控制能力，也不证明 provider 可连通。

<a id="vl-des-order"></a>
### 最后 flag 胜出

沿用[结构](#结构)第 1 项：parse_options 顺序写入同一 llm_mode 字段；两种顺序及重复 flag 应分别验证。不得将 launcher 的参数集合套用为 direct viewer 合同。

<a id="vl-des-observer"></a>
### observer 证据隔离

沿用[表现与验收边界](#表现与验收边界)：Script 仅 observer/debug，direct Viewer 截图/日志不代签 launcher/Web gameplay 或 release。LLM 开启也只是配置前置；适用 candidate、环境、入口、场景和证据须独立核对。

<a id="vl-des-launcher"></a>
### launcher 无成功回退

沿用[结构](#结构)第 3 项：launcher stack/harness 的 --no-llm 为负向路径，报错并提示 direct observer/debug；game launcher 在运行入口检查 with_llm，未启用时返回 gameplay requires --with-llm 错误。这里区分 wrapper 的 --no-llm 与 game launcher 的 --with-llm，不新增或声称后者接受 --no-llm。页面/CTA 或恢复流程若变更，仍须视觉交互专业结论及 S6。

<a id="vl-des-legacy"></a>
### legacy 控制面拒绝

沿用[结构](#结构)第 4 项：release/node 类参数拒绝并指向 chain runtime；--runtime-world 的专用错误说明其已移除、应去掉该 flag；不把所有错误文案混为同一迁移目标。既有链状态/提交连接参数不等于 viewer 拥有链控制面。

## 验证设计与可追溯性

### 11.1 验证映射表

本表是持续验证方法。下列 test source 均为固定源码中的定义，不表示本轮执行；本轮不运行 Rust、launcher、provider 或浏览器。

行为变更时由对应 owner 在实际候选运行适用套件，并保留 source/tested tree、命令、配置、环境、退出码与原始 artifact；缺前置记 blocked/inconclusive，真实断言违反记 failed。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [默认模式合同](oasis7-viewer-live-decision-mode.prd.md#vl-decision-contract) | [默认与映射](#vl-des-default) | 默认/显式 LLM 与 Script 映射 | [S1 binary 测试入口](../../../testing-manual.md)中的 `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs` inline tests：parse_options_defaults、parse_options_reads_custom_values 仅断言 true；在适用源码候选以 env -u RUSTC_WRAPPER cargo test -p oasis7_viewer_runtime --bin oasis7_viewer_live parse_options_ 运行解析层回归；--no-llm=false 与枚举映射直接断言仍待补，先静态核对 parse_options / initialize_viewer_server | 当前 PR 的命令/退出码与断言清单，缺失用例记录 | 未运行；现有 true 断言不能代签 false、枚举、provider 或 gameplay |
| [顺序规则](oasis7-viewer-live-decision-mode.prd.md#vl-last-flag) | [flag 顺序](#vl-des-order) | 最后 flag 胜出 | [S1 binary 测试入口](../../../testing-manual.md)所列 `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs` 的 parse_options线性赋值审读；planned 解析用例分别输入 --llm --no-llm、--no-llm --llm 与重复 flag，核对最终布尔值；目前该文件未见这些直接断言，须由 runtime owner 补齐后运行同一 bin 解析测试 | 当前 PR 的各顺序输入/预期/实际及新增测试定位 | 源码推理不等于顺序测试已覆盖或已通过 |
| Trace refs: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264；[observer 证据](oasis7-viewer-live-decision-mode.prd.md#vl-observer-evidence) | [证据隔离](#vl-des-observer) | 两类模式与入口证据不得代签 | 按 [Viewer 手册证据边界](../../world-simulator/viewer/viewer-manual.manual.md#证据边界)与同手册“进程与 Launcher 边界”逐条核 candidate/source、入口、LLM 配置、截图/日志的 claim；涉及 Web 时按总测试手册 S6 选择真实场景与环境 | 当前 PR / QA 同候选证据索引与 claim 审读，保留 unknown/blocked | 无 UI/DOM/browser、新 gameplay、parity 或 release 通过声明；历史截图不升级 |
| Trace refs: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270；[launcher 失败边界](oasis7-viewer-live-decision-mode.prd.md#vl-launcher-product-boundary) | [launcher 拒绝](#vl-des-launcher) | 无 LLM 不能成功 fallback | 静态核对 `scripts/run-launcher-stack.sh`、`scripts/worktree-harness.sh`的 --no-llm guard 与 `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs`的 with_llm guard；planned 在隔离候选环境分别验证 wrapper 非零退出、direct observer/debug 提示与未启动玩法；game launcher 拒绝未知 --no-llm 不能代替 with_llm=false guard 测试；表面变化按 [S6](../../../testing-manual.md#testing-s6-qa-receiver) | 当前 PR 三个入口分别归档退出码、stderr、启动副作用检查；S6 另记实际证据 | 未执行负向启动或 game launcher guard 直接测试；不证明 active provider、所有错误恢复或 UI |
| [legacy 拒绝](oasis7-viewer-live-decision-mode.prd.md#vl-legacy-control-rejection) | [单一控制面](#vl-des-legacy) | 旧控制面参数被拒绝且指引准确 | [S1 binary 测试入口](../../../testing-manual.md)中的 `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs` inline tests：parse_options_rejects_legacy_node_flags、parse_options_rejects_legacy_node_prefix_flags、parse_options_rejects_runtime_world_alias、parse_options_rejects_release_config_flag、parse_options_rejects_no_runtime_world；适用候选运行上述 bin parse_options_ 回归并与 print_help / REMOVAL_HINT / RUNTIME_ALIAS_REMOVAL_HINT 对账 | 当前 PR 的精确测试名、退出码、错误文案及帮助/手册审读 | 未运行；样例断言不穷尽所有 node-* 参数，亦不证明迁移后的 chain runtime 正确 |

剩余风险：默认及 legacy 现有测试定义不能填补 --no-llm、顺序和 launcher guard 的直接断言缺口。后续行为变化由 runtime/launcher owner 补齐，QA 按实际入口与 S6 条件收证；本追踪补充不修测试、不升级原里程碑、产品双状态或发布结论。
