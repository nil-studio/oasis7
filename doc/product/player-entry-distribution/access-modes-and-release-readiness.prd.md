# 访问模式与发行就绪

## 文档身份

- 所属产品模块：玩家接入与发行
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-20
- 公开状态权威：[`README.md`](../../../README.md)
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文是长期产品分册，承载玩家访问模式、入口能力等价、可解释失败恢复以及发行升阶的组合承诺。它不保存历史阶段 verdict、协议字段、命令、样本阈值、任务状态或渠道执行记录。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 适用边界：仅豁免重复的产品体验配对 design；专业系统设计仍按各功能状态和准确承接独立判定，不因此变为“不适用”。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 以入口能力、证据边界和失败恢复为单一产品合同；它不新增独立的信息架构或交互状态编排。
- 历史设计判定记录：历史分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，历史闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
## 1. 产品目标

玩家能够选择一个被明确支持的入口，理解该入口当前是可玩、仅观察还是被阻塞，并在同一版本和模式下完成发现、进入、核验、失败恢复与继续游玩。团队只有在同一候选版本的入口、运行恢复、QA 和公开沟通证据共同成立后，才能评估扩大阶段或对外 claim。

## 2. 玩家访问模式

- 正式 primary mode 只有 `viewer` 与 `pure_api`。前者提供 Web/UI 表面，后者提供无 UI 的正式玩家入口。
- alias、provider、execution lane、deployment、hosted session 或兼容路径不会形成第三种玩家模式，也不代表更高发布等级；其中 `software_safe` 仅是 `viewer` 的兼容 alias，不是独立玩家模式、发行等级或 claim / evidence lane。
- 每份可玩性、parity、observer 或 blocked 结论必须绑定一个 primary mode；`viewer` 与 `pure_api` 的证据不得互相代签。
- `pure_api` 不是协议探针或自动化执行 lane 的别名。它只有在专业域规定的正式玩法前置与独立证据成立时，才能声明为可玩。

### 2.1 Launcher 交付表面边界

Native 或 Web Launcher 是已声明 `viewer` / `pure_api` 路径的交付、控制与过渡表面，不是第三种玩家模式、发行等级或独立可玩性证明。首次使用时，玩家的窄承诺是：能够理解当前入口所服务的 primary mode、看到真实的 `blocked` / `observer` 状态，并获得不伪装为成功的可恢复下一步。稳定的首次引导与配置修复产品要求由 [Launcher 引导配置与可用性 PRD](../../world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.prd.md) 承接；具体 UI、控制面、请求状态、持久化、运行时恢复与验证契约继续由 [`doc/world-simulator/launcher/README.md`](../../world-simulator/launcher/README.md) 及其专业域文档维护。

该承诺不声称当前已实现某个引导步骤、自动修复、fallback、配置保存或持续运行结果。成功打开 Launcher、完成本地 fallback、探针/console 可用或单次启动成功，都不能反向证明声明的 primary mode 已经可玩，也不能扩大公开发行 claim。

### 2.2 受控 Launcher 动作

当当前专业 authority 支持时，Launcher 可让玩家对配置、设置、反馈或转账等受控动作看到真实的 `executable`、`blocked`、`result` 与恢复下一步；不同平台可以使用不同输入字段、存储或控制面适配。该产品边界要求语义真实而非界面同一：不得把浏览器存储或控制面可达解释为 WASM authority、授权、安全持久化或运行时成功；也不得把 runtime 的 submit acceptance/action ID 解释为 settlement、最终确认、完整持久历史、100% parity、primary-mode playability 或发行就绪。当前专业实现边界见 [跨表面受控动作契约](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md)。

## 3. 入口能力等价

`viewer` 与 `pure_api` 可以采用不同表现方式，但必须消费同一权威世界事实和玩家语义。决定持续游玩的能力不能因入口不同而丢失：

- 当前阶段、主目标、进度、主要 blocker 与下一步。
- 受支持的核心行动、接受或拒绝、主要世界后果与恢复动作。
- 首局后的阶段承接、首次持续能力与中循环继续路径。
- 断线、重连或切换客户端后的目标、最近结果和下一步恢复。

UI 私有聚合或 API 客户端自行推导不能成为第二事实源。协议可连、能够读取 snapshot、单次 step 成功或 observer smoke 都不能单独证明正式可玩或能力等价。

## 4. 可解释失败与恢复

- 入口前置缺失、身份或会话失效、provider 初始化失败、模式不受支持或后端不可用时，玩家必须得到准确的 blocked/observer 分类、原因和恢复下一步。
- 恢复可以是 reconnect、re-register、re-auth、选择受支持模式、修复配置或稍后重试；不得静默恢复旧权限，也不得把 fallback 包装成原入口成功。
- 无 LLM、debug、probe 或 observer 路径只能支持对应的受限结论，不能代签 `pure_api` 或 `viewer` 的正式可玩性。
- 对外说明先纠正当前状态，再给最小解释和正式反馈/升级入口；不得用模糊措辞把技术通路包装成已发布体验。

## 5. 统一候选门禁与公开口径

- 阶段或公开 claim 的评估必须绑定同一候选版本，而不是拼接不同版本、不同模式或不同专题的局部 green。
- 候选证据至少覆盖适用的 `viewer`、`pure_api`、权威运行与恢复、发行资产、QA 汇总及 LiveOps/公开说明同步。
- 候选缺口按发布影响、闭环依赖和证据/owner 就绪度排序；局部或历史 green 不能据此获得更高优先级或代签整体就绪。
- 任一硬门失败、缺证或只具备历史样本时，保持较低承诺并记录 blocker；不能用 source-tree pass、单专题 pass 或旧 go 记录代签整体验证。
- release gate 通过只是升阶前提，不自动改变当前阶段或公开 claim。正式变更还需要产品决策、QA 结论、LiveOps 同步，并最终反映到根 `README.md`。
- 根 `README.md` 始终是当前公开状态与 claim envelope 的唯一权威；本分册不固化某次 Alpha、Beta 或 preview verdict。

## 5.1 叶级产品要求与验收

<a id="req-entry-candidate-001"></a>
### REQ-ENTRY-CANDIDATE-001：候选门禁必须绑定同一版本的跨域证据

Trace ID: O7-REQ-6b906d19-8858-4b62-9526-5952cfc9ad8b

- 要求：阶段或公开 claim 的候选门禁必须把 primary mode、适用入口、权威 runtime/P2P 执行、恢复路径、长期经济审计、发行资产、QA 汇总与公开 claim 输入绑定到同一候选版本和证据窗口。任一输入缺失、来自不同候选、只具备局部/历史 green 或无法证明跨域对应关系时，产品结果只能保持 blocked、observer 或更窄的目标性结论；本要求不生成当前 release verdict。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`README.md`](../../../README.md)
- 验收：[AC-ENTRY-CANDIDATE-001](#ac-entry-candidate-001)


- 功能点：[REQ-ENTRY-CANDIDATE-001](#req-entry-candidate-001)；验收 [AC-ENTRY-CANDIDATE-001](#ac-entry-candidate-001)。
- 适用范围：同候选的两种 primary mode、runtime/P2P 恢复、经济、发行资产及公开口径组合门禁；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[官网专业设计](../../site/github-pages/public-site-discovery-and-showcase.design.md#des-site-download)已承接：版本、日期、Release 说明、实际资产与 checksum 以同一候选快照原子更新；失败不混版；同 Trace 五列承接与六列验证限定网站子范围。
- 实现状态：部分实现
- 实现与验证依据：候选 bundle 已固定 git commit、runtime/world/governance/evidence refs 并校验文件/目录 hash；release gate 分步骤保留 skipped/dry-run 的 claim 边界。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/scripts/release-candidate-bundle.sh)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/scripts/release-candidate-bundle-smoke.sh)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/scripts/release-gate.sh)；[核验任务](https://github.com/eng-cc/oasis7/issues/4696)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，两种 primary mode、恢复、长期经济、全部发行资产和公开 claim 的同候选/同窗口语义 join 尚未核清；bundle存在不是完整发行门禁通过。

- 设计复核日期与基线：2026-10-15；`dffd797f` 文档候选；设计剩余：两 primary mode、跨域 QA/经济/恢复及完整 release gate 不由网站下载片段覆盖。不改变本卡固定源码与已取得实现范围。

<a id="ac-entry-candidate-001"></a>
### AC-ENTRY-CANDIDATE-001：局部或跨候选证据不能代签整体验收

- 覆盖要求：REQ-ENTRY-CANDIDATE-001
- 给定：一个候选版本及其入口、runtime/P2P、恢复、经济审计、发行资产、QA 和公开 claim 证据，另有局部 green、历史证据或跨候选混合的对照样例。
- 当：产品评估阶段/公开 claim 是否满足候选门禁。
- 则：只有同一候选且证据窗口、primary mode、适用入口和跨域对应关系完整的样例，才可形成候选门禁输入；任何缺证、候选漂移、局部/历史 green 或对应关系不明的样例保持 blocked、observer 或较窄的目标性结论，不能代签当前 release readiness、公开可用性或阶段自动升级。专业 runtime/P2P/恢复、经济、QA、LiveOps 与根 README 仍各自拥有实现、证据和公开状态 authority。

<a id="req-entry-mode-001"></a>
### REQ-ENTRY-MODE-001：每个入口必须绑定唯一 primary mode

Trace ID: O7-REQ-f3f198ce-2afd-4487-a99e-9f371bdd8264

- 要求：玩家入口必须明确其 primary mode、当前阶段、受支持能力和证据范围；viewer、pure_api、observer 或 blocked 结论不得由 alias、provider、deployment 或 session context 隐式推导或互相代签。
- 验收：AC-ENTRY-MODE-001


- 功能点：[REQ-ENTRY-MODE-001](#req-entry-mode-001)；验收 [AC-ENTRY-MODE-001](#ac-entry-mode-001)。
- 适用范围：viewer/pure_api 入口的唯一模式、阶段、能力及独立证据表达；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[逐账号首次绑定与恢复](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)经专业 REQ/AC 承接同世界首次注册、已绑定账号返回及其 blocked/未知结果的逐入口证据和失败恢复；这是后继设计目标，不是已开放准入。
- 本次补充系统设计依据：[Viewer 决策模式](../../p2p/viewer-live/oasis7-viewer-live-decision-mode.design.md#vl-des-observer)仅承接direct viewer Script observer/debug 与 launcher/gameplay/release 的证据隔离；其余原剩余范围保留，系统设计仍为部分覆盖，原实现状态和历史核对基线不变。
- 实现状态：未核实
- 实现与验证依据：release snapshot → 页面阶段/平台展示；Launcher no-LLM guard → viewer URL。 下载 DTO 实际只携带版本/资产；render_mode 固定 viewer；可见网页明确仅验证面，但未形成 phase/capability/evidence 的统一模式事实。 [site/assets/release-candidate.js:3–26](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/assets/release-candidate.js#L3-L26)；[crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs:335–344](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L335-L344)；[crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs:966–985](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L966-L985)；[site/index.html:383–387](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/site/index.html#L383-L387)
- 核对日期与基线：2026-10-17；源码 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，本轮固定源码/操作记录只读复核；未运行安装、浏览器、模型或业务验收。
- 剩余范围：primary mode 与适用能力/证据的逐入口显式绑定、pure_api 独立消费者仍未核清。保留未核实；已定位两个真实生产入口的表达边界。

- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

<a id="ac-entry-mode-001"></a>
### AC-ENTRY-MODE-001：入口证据独立证明模式能力

- 覆盖要求：REQ-ENTRY-MODE-001
- 场景与结果：同一候选版本分别核对入口与 primary mode；玩家能辨认可玩、仅观察、blocked 和受支持的下一步，局部 launcher、浏览器、自动化 green 或另一模式证据不会扩大当前 claim。
- 证据边界：模式分类、runtime/Viewer 证据和公开状态由对应专业 authority、QA 与根 README 共同确认；本产品层不保存候选 verdict。

<a id="req-entry-mode-002"></a>
### REQ-ENTRY-MODE-002：失败入口必须给出真实恢复路径

Trace ID: O7-REQ-94ed85c5-c05d-4385-a919-687bfd66b270

- 要求：入口前置缺失、身份/会话失效、provider 初始化失败、模式不受支持或后端不可用时，产品必须如实呈现 blocked/observer 分类、主要原因和适用恢复路径，不得用 fallback 或自动重试伪装成功。
- 验收：AC-ENTRY-MODE-002


- 功能点：[REQ-ENTRY-MODE-002](#req-entry-mode-002)；验收 [AC-ENTRY-MODE-002](#ac-entry-mode-002)。
- 适用范围：前置、身份/会话、provider、模式和后端失败时的分类与真实恢复；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[逐账号首次绑定与恢复](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)经专业 REQ/AC 承接同世界首次注册、已绑定账号返回及其 blocked/未知结果的逐入口证据和失败恢复；这是后继设计目标，不是已开放准入。
- 本次补充系统设计依据：[Viewer 决策模式](../../p2p/viewer-live/oasis7-viewer-live-decision-mode.design.md#vl-des-launcher)仅承接launcher 无 LLM fail-fast、禁止 Script 成功 fallback 及 direct observer/debug 提示；其余原剩余范围保留，系统设计仍为部分覆盖，原实现状态和历史核对基线不变。
- 实现状态：部分实现
- 实现与验证依据：Launcher 对配置无效、禁用和启动失败给出 ConfigError/Disabled/Unreachable；会话请求失败清空旧 authority 快照。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_client_launcher/src/main_chain_status.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4696)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，全部 viewer/pure_api 的 provider、身份、模式与后端失败及真实恢复尚未核清；局部错误分类不证明完整可玩路径。

<a id="ac-entry-mode-002"></a>
### AC-ENTRY-MODE-002：blocked 与可玩不会被静默互换

- 覆盖要求：REQ-ENTRY-MODE-002
- 场景与结果：代表性前置失败和 fallback 样例保持 blocked/observer 语义；玩家可选择修复配置、重新验证、选择受支持模式、等待或安全返回，重试不会恢复旧权限或制造世界进展。
- 证据边界：具体错误、会话、配置和恢复动作由 Launcher/runtime/Viewer/WASM/QA authority 定义；产品层不冻结状态字段或命令。

## 6. 组合验收

### 6.1 受控 Limited Preview

- Limited preview 是面向候选版本的受控、可纠偏、可撤回体验，不等于 public launch、广泛可用或阶段自动升级。
- 每轮 preview 必须绑定同一候选版本、primary mode、受支持入口、claim envelope 与正式反馈入口；入口不可达或被迫 fallback 时必须如实标记 blocked，不能改写为成功。
- 反馈按 `Blocking / Opportunity / Idea` 分类，并记录 owner、处置结论与下一步。可复现的 blocking 信号必须让同一候选重新进入受影响专业域与 QA gate，旧 gate pass 不能代签。
- 正式反馈入口必须如实呈现当前可用性；可提交时，玩家能够确认提交结果；入口不可用或提交未完成时，必须如实呈现且不得计作成功。只有当前专业 authority 与已验证入口表面已经支持时，才可向玩家给出恢复下一步；本分册不声称当前尚未实现的禁用 CTA 状态恢复引导。该承诺不包含公开反馈账本、必达交付、保存期限、审核或调解、人工支持或逐条答复。
- claim drift 必须在同轮纠正并留下记录；单次 callout、互动数量或无阻塞反馈都不能单独证明发行就绪。
- QA 可以建议 pause、conditional 或 downgrade；producer 基于反馈与 QA 结论决定 continue / hold / reassess。渠道切换不改变阶段，公开状态仍只由根 `README.md` 拥有。
- 收集反馈时只索取复现和分流所需的最小信息；不得把账号、私密渠道内容、敏感配置/凭据或其他不必要的个人信息写入公开证据。

- AR-1：所有入口和证据都能归一到 `viewer` 或 `pure_api`，不存在由 alias、provider、deployment 或 session context 派生的第三种模式。
- AR-2：同一候选版本分别证明 `viewer` 与 `pure_api` 的阶段、目标、阻塞、下一步、核心动作、主要因果与重连恢复；两种模式证据不互相代签。
- AR-3：blocked/observer 样例能说明真实原因和恢复路径，且不会继续保留错误的 playable/parity 结论。
- AR-4：统一候选门禁把入口、权威运行与恢复、发行资产、QA 和公开 claim 绑定到同一版本；局部或历史 green 无法代签。叶级边界由 [REQ-ENTRY-CANDIDATE-001](#req-entry-candidate-001) / [AC-ENTRY-CANDIDATE-001](#ac-entry-candidate-001) 冻结。
- AR-5：公开口径变更可追踪到产品决策、专业验证、QA 结论、LiveOps 同步和根 README；gate pass 不自动等于阶段升级。
- AR-6：适用的 Launcher 受控动作如实区分可执行、阻断与结果，并给出对应恢复下一步；平台字段或适配差异、submit acceptance、近期历史与控制面可达均不被外推为跨端完全一致、结算、持久性、可玩性或发行结论。
- LP-1：每轮 preview 都能追踪同一候选、primary mode、受支持入口、claim envelope 与反馈入口，且不会被表述为公开发布或自动升阶。
- LP-2：声明入口能进入对应 primary mode；blocked、observer 或 fallback 不会被计作成功，并由 Viewer S6 及适用的 S9/S10、发行资产证据验证。
- LP-3：每条有效信号都有分类、owner、复现或处置结论和下一步；自动化 green 不能替代真实反馈 trace。
- LP-4：可复现 blocker 会使对应 lane 失效并触发受影响专业域和当前候选 gate 重验，而不是继续复用旧 verdict。
- LP-5：continue / hold / reassess 决策能追踪 QA、LiveOps、producer 与根 README；渠道故障或切换不会静默改变阶段。

### 6.2 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| AR-1 | producer_system_designer / viewer_engineer / qa_engineer | PRD-WORLD_SIMULATOR-039 / PRD-WORLD_SIMULATOR-041 / PRD-WORLD_SIMULATOR-046 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/testing/prd.md` | primary mode taxonomy 与 claim/evidence 归一化审计 | test_tier_required |
| AR-2 | gameplay_designer / viewer_engineer / qa_engineer | PRD-GAME-008 / PRD-WORLD_SIMULATOR-039 / PRD-WORLD_SIMULATOR-041 / PRD-WORLD_SIMULATOR-046 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | Viewer S6 与 pure API 独立长玩/parity 对账，覆盖阶段承接与重连恢复 | test_tier_full |
| AR-3 | viewer_engineer / agent_engineer / qa_engineer | PRD-WORLD_SIMULATOR-016 / PRD-WORLD_SIMULATOR-039 / PRD-WORLD_SIMULATOR-041 / PRD-WORLD_SIMULATOR-046 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 前置失败、blocked/observer 分类及可执行恢复路径证据 | test_tier_required |
| AR-4 | runtime_engineer / viewer_engineer / qa_engineer / liveops_community | PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-042 / PRD-WORLD_SIMULATOR-045 / PRD-TESTING-003 | `doc/world-runtime/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 同候选版本完整 release gate；适用时包含 S6、S9/S10 与真实发行资产 | test_tier_full |
| AR-5 | producer_system_designer / qa_engineer / liveops_community | PRD-WORLD_SIMULATOR-042 / PRD-WORLD_SIMULATOR-043 / PRD-TESTING-003 | `README.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 产品决策、QA 结论、LiveOps 同步与根 README claim 变更追踪 | test_tier_required |
| LP-1 | producer_system_designer / viewer_engineer / liveops_community / qa_engineer | PRD-GAME-010 / PRD-WORLD_SIMULATOR-042 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 候选、模式、入口、claim 与反馈入口的同轮绑定审计 | test_tier_required |
| LP-2 | viewer_engineer / qa_engineer | PRD-GAME-010 / PRD-WORLD_SIMULATOR-039 / PRD-WORLD_SIMULATOR-041 / PRD-WORLD_SIMULATOR-046 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | Viewer S6 与适用的 S9/S10、真实发行资产；blocked/fallback 负例 | test_tier_full |
| LP-3 | liveops_community / qa_engineer | PRD-GAME-010 / PRD-WORLD_SIMULATOR-043 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | LiveOps / GitHub 信号分类、owner、处置与下一步 trace | test_tier_required |
| LP-4 | gameplay_designer / runtime_engineer / viewer_engineer / qa_engineer / producer_system_designer | PRD-GAME-010 / PRD-WORLD_SIMULATOR-042 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | blocker 对应专业回归与当前候选 gate 重验 | test_tier_full |
| LP-5 | producer_system_designer / qa_engineer / liveops_community | PRD-GAME-010 / PRD-WORLD_SIMULATOR-042 / PRD-WORLD_SIMULATOR-043 / PRD-TESTING-003 | `README.md`; `doc/game/prd.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | continue / hold / reassess 决策及公开 claim 同步追踪 | test_tier_required |

## 7. Non-Goals

- 不定义协议 schema、客户端字段、状态枚举或具体错误码。
- 不冻结趋势阈值、运行时长、样本数量、性能预算或自动化命令。
- 不保存某次候选版本的 pass/block、阶段判断或任务完成状态。
- 不替代发行 runbook、渠道文案模板、事故处理或社区反馈记录。

## 8. 未决问题与假设

- 未决：每个候选版本的具体入口、primary mode、恢复证据和公开 claim 仍由 runtime、Viewer、QA 与 LiveOps 在同一证据窗口内确认；本分册不预先宣称任何当前模式可玩或可发行。
- 假设：当入口证据缺失、状态冲突或恢复路径未经当前 authority 支持时，产品结论保持 blocked/observer 或更窄范围，并等待重新验证、修复或重新评估。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-ENTRY-CANDIDATE-001](#req-entry-candidate-001) / [AC-ENTRY-CANDIDATE-001](#ac-entry-candidate-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`README.md`](../../../README.md) | 同候选 primary mode/入口、runtime/P2P、恢复、长期经济审计、发行资产、QA 与公开 claim 的完整证据窗口；缺证、候选漂移或局部/历史 green 只能保持 blocked/observer/较窄目标性结论，不生成当前 release verdict | `test_tier_full` |
| [REQ-ENTRY-MODE-001](#req-entry-mode-001) / [AC-ENTRY-MODE-001](#ac-entry-mode-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 待执行方法：按 AC 分别核对 viewer/pure_api 当前入口及证据，加入 alias/另一模式代签负例；待取得逐入口记录；导航关系不等于通过证据 | `test_tier_required` |
| [REQ-ENTRY-MODE-002](#req-entry-mode-002) / [AC-ENTRY-MODE-002](#ac-entry-mode-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 待执行方法：按 AC 注入前置缺失、会话失效及后端不可用，核对 blocked/observer 和恢复下一步；待取得运行/表面记录；导航关系不等于通过证据 | `test_tier_required` |
