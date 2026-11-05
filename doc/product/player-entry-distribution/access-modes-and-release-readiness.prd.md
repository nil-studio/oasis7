# 访问模式与发行就绪

## 文档身份

- 所属产品模块：玩家接入与发行
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-10-19
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

玩家能够选择明确支持的入口，理解其当前是可玩、仅观察还是被阻塞，并在同一版本和模式下完成发现、进入、核验、失败恢复与继续游玩。团队只有在同一候选版本的入口、运行恢复、QA 和公开沟通证据共同成立后，才能评估扩大阶段或对外 claim。

## 2. 玩家访问模式

- 正式 primary mode 只有 `viewer` 与 `pure_api`。前者提供 Web/UI 表面，后者提供无 UI 的正式玩家入口。
- alias、provider、execution lane、deployment、hosted session 或兼容路径不会形成第三种玩家模式，也不代表更高发布等级；其中 `software_safe` 仅是 `viewer` 的兼容 alias，不是独立玩家模式、发行等级或 claim / evidence lane。
- 每份可玩性、parity、observer 或 blocked 结论必须绑定一个 primary mode；`viewer` 与 `pure_api` 的证据不得互相代签。
- `pure_api` 不是协议探针或自动化执行 lane 的别名。它只有在专业域规定的正式玩法前置与独立证据成立时，才能声明为可玩。

### 2.1 Launcher 交付表面边界

Native 或 Web Launcher 是已声明 `viewer` / `pure_api` 路径的交付、控制与过渡表面，不是第三种玩家模式、发行等级或独立可玩性证明。首次使用时，玩家的窄承诺是：能够理解当前入口所服务的 primary mode、看到真实的 `blocked` / `observer` 状态，并获得不伪装为成功的可恢复下一步。稳定的首次引导与配置修复产品要求由 [Launcher 引导配置与可用性 PRD](../../world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.prd.md) 承接；具体 UI、控制面、请求状态、持久化、运行时恢复与验证契约继续由 [`doc/world-simulator/launcher/README.md`](../../world-simulator/launcher/README.md) 及其专业域文档维护。

该承诺不声称当前已实现某个引导步骤、自动修复、fallback、配置保存或持续运行结果。成功打开 Launcher、完成本地 fallback、探针/console 可用或单次启动成功，都不能反向证明声明的 primary mode 已经可玩，也不能扩大公开发行 claim。

### 2.2 受控 Launcher 动作

在当前专业 authority 支持时，Launcher 可向玩家呈现配置、设置、反馈或转账等受控动作的真实 `executable`、`blocked`、`result` 与恢复下一步；不同平台可以使用不同输入字段、存储或控制面适配。该产品边界要求语义真实，而非界面相同：不得把浏览器存储或控制面可达解释为 WASM authority、授权、安全持久化或运行时成功；也不得把 runtime 的 submit acceptance/action ID 解释为 settlement、最终确认、完整持久历史、100% parity、primary-mode playability 或发行就绪。当前专业实现边界见 [跨表面受控动作契约](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md)。

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

## 4.1 Viewer 持续响应与资源等待

本节适用于受支持浏览器中的 `viewer`，包括 Hosted 入口；不替代身份、世界准入或权威回执合同。具体设备、网络、场景、指标阈值与采样规则由[系统性能设计](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#viewer-performance-contract)维护；未达预算只能报告对应体验缺口，不据此改变公开发行状态。

<a id="req-entry-performance-001"></a>
### REQ-ENTRY-PERFORMANCE-001：世界更新期间保持玩家操作响应

Trace ID: O7-REQ-7e64b369-3c32-4671-a1af-205bc5a75251

- 要求：在声明的设备与世界负载内，观察世界、切换目标、移动视角、打开命令及输入草稿必须保持可响应。只变化的显示内容触发相应刷新，背景动画、无关诊断和相同世界数据不得使玩家持续等待。减少展示成本不得丢失受权限约束的实体删除、目标/阻塞、动作结果或最近权威进度；过载时可减少装饰与可见细节，并明确显示正在追赶或数据不可用，不能把旧画面表示成实时结果。
- 验收：[AC-ENTRY-PERFORMANCE-001](#ac-entry-performance-001)
- 系统设计状态：已覆盖
- 系统设计依据：[显示投影与调度](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#viewer-render-projection)。
- 核对日期与基线：2026-10-19；`ebaefe593fc81806d01e10aa8bfe09bd127fe7e6`；静态源码核对与本次目标设计，不包含实现或部署验收。
- 实现状态：部分实现
- 实现与验证依据：2026-10-19；在设计合入基线 `fb15585d8811ef5c7ea010139f25908812289213` 上实现有界显示白名单、普通 JSON/版本化 Rust 输入、同帧最新更新及相同显示派生复用；Host 回归覆盖 telemetry/tick、实体删除、可见性失效及回执处理。真实 Chrome 在 1440×1000、390×844 稀疏只读样例中显示 WebGL 画布并完成选择；具体候选及局部测量见 [实现 PR #4909](https://github.com/nil-studio/oasis7/pull/4909)，不代签完整预算。
- 后续实现范围：稳定身份排序与关键语义优先裁剪已补齐，输入与输出均会缩减可选标记并保留源计数；可见标签总数 100、每项 128 个 Unicode 标量；Targets 分页限制同时挂载的行数，完整授权搜索、计数与选择仍可达。
- 剩余范围：常规/高负载及受控更新场景的完整同候选性能验收；必需语义无法容纳时明确 unavailable，不升级现有模式就绪结论。

<a id="ac-entry-performance-001"></a>
### AC-ENTRY-PERFORMANCE-001：持续响应与事实一致性

- 对应要求：[REQ-ENTRY-PERFORMANCE-001](#req-entry-performance-001)。

在系统设计的常规与高负载场景、桌面与窄屏中，完成上述交互并达到响应预算；重复相同快照不产生重复全量转换。故障、权限变化及重连后仍正确呈现删除/不可见、结果与数据新鲜度。流畅动画或单张截图不能代签交互响应和事实一致性。

<a id="req-entry-performance-002"></a>
### REQ-ENTRY-PERFORMANCE-002：首次加载可解释，同版本重入复用已验证资源

Trace ID: O7-REQ-25030f5c-c246-4b72-b637-de9dc6b520ae

- 要求：玩家先看到可理解的入口、资源准备阶段及恢复下一步；世界图形未就绪时不得伪装为可玩。相同版本再次进入应复用可用且与当前发布绑定的已验证公共图形资源，不应无理由重复传输大文件。慢网、离线、资源缺失或完整性失败时，已有安全入口、诊断和退出仍可操作；重试只恢复资源准备，不重新提交游戏动作或改变会话权限。
- 验收：[AC-ENTRY-PERFORMANCE-002](#ac-entry-performance-002)
- 系统设计状态：已覆盖
- 系统设计依据：[资源交付与恢复](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#viewer-payload-delivery)。
- 核对日期与基线：2026-10-19；`ebaefe593fc81806d01e10aa8bfe09bd127fe7e6`；静态源码核对与本次目标设计，不包含实现或部署验收。
- 实现状态：部分实现
- 实现与验证依据：2026-10-19；上述设计合入基线上实现精简 WASM feature/发布符号、SHA-256 寻址和 gzip artifact、服务器编码协商与 immutable 公共资源缓存；稳定 JS/CSS 保持 revalidation，HTML 保留原运行时注入。Loader 每次验证解码 bytes，仅损坏缓存自动重取一次，并提供资源阶段和失败后重新尝试。打包与 HTTP/loader 回归验证内容、编码及失败边界，具体候选及体积见 [实现 PR #4909](https://github.com/nil-studio/oasis7/pull/4909)。
- 后续实现范围：每 attempt 的 manifest/payload/reload 下载和读 body 共享 30 秒预算，hash/compile/init 共享独立 10 秒预算，重试使用独立实例并拒绝过期结果；同步初始化超时只能在返回后检测，不能据此承诺阻塞期间界面仍及时响应。
- 剩余范围：真实部署的冷暖缓存/版本切换/离线与受控网络验收；不得将本地体积或设计预算当作当前网站性能承诺。

<a id="ac-entry-performance-002"></a>
### AC-ENTRY-PERFORMANCE-002：冷暖加载与安全恢复

- 对应要求：[REQ-ENTRY-PERFORMANCE-002](#req-entry-performance-002)。

冷启动、同版本重入、版本切换、缓存被淘汰、离线及损坏资源分别满足系统预算或显示真实的失败/恢复；版本切换不混用旧资源，校验失败不进入图形就绪。准备、图形就绪、身份就绪、世界同步与可提交动作分别核验；本地资源恢复不代签后端可用或世界结果。

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
- 系统设计状态：已覆盖
- 系统设计依据：[逐账号首次绑定与恢复](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)经专业 REQ/AC 承接同世界首次注册、已绑定账号返回及其 blocked/未知结果的逐入口证据和失败恢复；这是后继设计目标，不是已开放准入。 本轮 [Launcher 状态与双语文案](../../world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.design.md#launcher-uiux-state-copy)承接交付表面可见的模式与阶段边界；现有按钮只打开真实支持的目标，不新增 pure_api 启动入口，也不从链 observer 推导玩家模式。 [Viewer入口/会话/世界组合](../../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md#uiux-entry-session-world)与[Live/Playback上下文](../../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md#uiux-live-playback-context)定义viewer的当前身份、阶段和受限能力；[正式API入口适用映射](../../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability)逐声明pure_api组合接收唯一mode、实际消费者、协商能力和独立证据。SDK、CLI、HTTP等组件与primary mode分开，不以局部可连通缩小原承诺。
- 验证方法：[API逐入口同候选验证](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-primary-entry-validation)、[Viewer QA清单](../../world-simulator/viewer/viewer-manual.manual.md#target-qa-evidence-checklist)和[Launcher LG-01～06](../../testing/manual/launcher-guided-uiux-acceptance.manual.md)分别核能力/证据、原请求、故障分类及合法下一步；方法与静态审读不表示运行通过。
- 历史局部系统设计依据：[Viewer 决策模式](../../p2p/viewer-live/oasis7-viewer-live-decision-mode.design.md#vl-des-observer)仅承接direct viewer Script observer/debug 与 launcher/gameplay/release 的证据隔离；该次仅核该子范围，故当时保留部分覆盖；原实现状态和历史核对基线不变。本轮完整设计接收另见本卡当前依据与方法。
- 实现状态：部分实现
- 实现与验证依据：直接子义务是受限入口的显式能力/失败边界：[launcher拒绝与observer下一步](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/scripts/run-launcher-stack.sh#L464-L467)在缺 LLM 时退出并给 direct observer/debug 下一步；[no-LLM参数](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs#L324-L328)经[direct配置消费者](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs#L185-L196)锁定 Script，[direct受限能力说明](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_live.rs#L568-L582)明确 observer/debug only、gameplay blocked；[game-launcher硬拒绝](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L340-L344)阻止 launcher 成功回退。此处不把缺 LLM、provider 或 deployment 推导成 primary mode。[viewer URL与deployment独立参数](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs#L967-L985)仅显式 viewer render 参数与 hosted context；另已核到真正[pure API签名动作与响应](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs#L213-L231)、[独立连接与HelloV2](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_client_cli/src/bin/oasis7_pure_api_client.rs#L550-L582)及[JSON消费者](https://github.com/nil-studio/oasis7/blob/bca169a97393da6cb9b8c0a28246a4efc6d813db/crates/oasis7_client_cli/src/bin/oasis7_pure_api_client/support.rs#L330-L341)，不能再将 pure_api 独立消费者整体列作未知，但这些协议能力仍非完整 primary-mode/阶段/证据分类。 本轮为静态实现认定，不记同候选运行 PASS。
- 核对日期与基线：2026-10-18；需求、设计映射与源码固定 main `bca169a97393da6cb9b8c0a28246a4efc6d813db`；逐 Git blob 与有效行号核验，仅静态文本审读。未运行安装、模型、Runtime、浏览器或业务/测试验收；测试定义、历史记录及其他候选 CI 不代签本候选。系统设计轴及原阶段、前置条件保持不变。
- 剩余范围：入口的唯一mode、阶段、实际能力/证据和失败恢复已有完整组合设计及独立方法。实现仍须逐一接通正式viewer/pure_api的当前事实、消费者与同候选输出；旧CLI的协商、actor、原件保存及原结果查询等具体差距沿API映射保留，不能写成仅欠运行测试。已有direct observer/debug、Launcher无LLM拒绝、HelloV2/JSON和签名动作只证明原固定子范围，不代签正式可玩、parity或发行；首次注册/返回仍遵守原后继目标及未开放准入条件。

- 本次实现复核：2026-10-17；需求与源码固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub固定文本/PR记录审读；覆盖本条原入口/阶段及验收范围，未执行浏览器、安装或业务验收；旧核对基线只保留历史语境。

- 历史Launcher局部设计核对：2026-11-05；已合入源码 `4f6f20ece923c5705e26ef879c2b83e0e637b3f9` 与当时文档候选，仅核 Launcher 引导表面的具体交接。该次仅接收Launcher子范围并保留部分覆盖；本轮已按上述专业条款接收完整跨入口/权限/凭据/世界组合。原实现状态及历史证据保留，未新增运行或真人验收。

- 本轮完整设计核对：2026-11-05；已合入源码 `c67298dcfc08ce0eaece293bd30c184980ac5327` 及本次API入口映射/方法候选。实际核对上述Viewer、Launcher、API和原操作消费者的完整适用关系；只接收设计与方法，原实现部分、历史证据和正式阶段前置保持，未新增运行或真人验收。

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
- 系统设计状态：已覆盖
- 系统设计依据：[逐账号首次绑定与恢复](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)经专业 REQ/AC 承接同世界首次注册、已绑定账号返回及其 blocked/未知结果的逐入口证据和失败恢复；这是后继设计目标，不是已开放准入。 本轮 [Launcher 状态与文案](../../world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.design.md#launcher-uiux-state-copy)及[失败恢复](../../world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.design.md#launcher-uiux-recovery)为该表面的真实 blocker 给出已有动作、双语提示及重验条件；无可执行恢复时保持明确受阻，不制造新的登录或 observer fallback。 [Viewer账号与世界恢复](../../world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md#uiux-entry-session-world)承接认证/绑定/局部列表失败与原认证尝试未知，保持账号、准入、控制和原操作的各自边界；[Viewer同步恢复](../../world-simulator/viewer/viewer-world-navigation-uiux.design.md#r-target-04)分别呈现连接、hello、snapshot、auth sync与可见计数，仅采用Runtime已发布恢复动作，不制造Agent；[正式API入口恢复](../../world-simulator/llm/external-agent-game-api.design.md#api-primary-entry-applicability)逐消费者复用原查询、精确继续和不可查询时的受阻下一步。模型/provider失败只阻断实际依赖组合，切入口或查询不能补签原件、恢复旧写权或消除未知。
- 验证方法：[API逐入口同候选验证](../../testing/manual/external-agent-runtime-contract-validation.manual.md#api-primary-entry-validation)、[Viewer QA清单](../../world-simulator/viewer/viewer-manual.manual.md#target-qa-evidence-checklist)和[Launcher LG-01～06](../../testing/manual/launcher-guided-uiux-acceptance.manual.md)分别核能力/证据、原请求、故障分类及合法下一步；方法与静态审读不表示运行通过。
- 历史局部系统设计依据：[Viewer 决策模式](../../p2p/viewer-live/oasis7-viewer-live-decision-mode.design.md#vl-des-launcher)仅承接launcher 无 LLM fail-fast、禁止 Script 成功 fallback 及 direct observer/debug 提示；该次仅核该子范围，故当时保留部分覆盖；原实现状态和历史核对基线不变。本轮完整设计接收另见本卡当前依据与方法。
- 实现状态：部分实现
- 实现与验证依据：Launcher 对配置无效、禁用和启动失败给出 ConfigError/Disabled/Unreachable；会话请求失败清空旧 authority 快照。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_client_launcher/src/main_chain_status.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4696)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：前置、身份/会话、provider、模式与后端故障已有逐入口的分类、原结果关联和安全恢复设计；正式viewer/pure_api各消费者的完整接线、真实失败组合、读权限变化及同候选恢复仍须实施和验证。当前CLI缺少的原件/增强能力适配保持明确，局部错误分类、正确目标合同或另一模式运行不代签完整可玩路径。

- 历史Launcher局部设计核对：2026-11-05；已合入源码 `4f6f20ece923c5705e26ef879c2b83e0e637b3f9` 与当时文档候选，仅核 Launcher 引导表面的具体交接。该次仅接收Launcher子范围并保留部分覆盖；本轮已按上述专业条款接收完整跨入口/权限/凭据/世界组合。原实现状态及历史证据保留，未新增运行或真人验收。

- 本轮完整设计核对：2026-11-05；已合入源码 `c67298dcfc08ce0eaece293bd30c184980ac5327` 及本次API入口映射/方法候选。实际核对上述Viewer、Launcher、API和原操作消费者的完整适用关系；只接收设计与方法，原实现部分、历史证据和正式阶段前置保持，未新增运行或真人验收。

<a id="ac-entry-mode-002"></a>
### AC-ENTRY-MODE-002：blocked 与可玩不会被静默互换

- 覆盖要求：REQ-ENTRY-MODE-002
- 场景与结果：代表性前置失败和 fallback 样例保持 blocked/observer 语义；玩家可选择修复配置、重新验证、选择受支持模式、等待或安全返回，重试不会恢复旧权限或制造世界进展。
- 证据边界：具体错误、会话、配置和恢复动作由 Launcher/runtime/Viewer/WASM/QA authority 定义；产品层不冻结状态字段或命令。

## 6. 组合验收

### 6.1 受控 Limited Preview

- Limited preview 是面向候选版本的受控、可纠偏、可撤回体验，不等于 public launch、广泛可用或阶段自动升级。
- 每轮 preview 必须绑定同一候选版本、primary mode、受支持入口、claim envelope 与正式反馈入口；入口不可达或被迫 fallback 时必须如实标记 blocked，不能改写为成功。
- 反馈入口与提交结果沿 [REQ-ENTRY-PREVIEW-FEEDBACK-001](#req-entry-preview-feedback-001)；分类、处置与 blocker 重验沿 [REQ-ENTRY-PREVIEW-FEEDBACK-002](#req-entry-preview-feedback-002)。两条保留本节既有受控范围和结果边界。
- claim drift 必须在同轮纠正并留下记录；单次 callout、互动数量或无阻塞反馈都不能单独证明发行就绪。
- QA 可以建议 pause、conditional 或 downgrade；producer 基于反馈与 QA 结论决定 continue / hold / reassess。渠道切换不改变阶段，公开状态仍只由根 `README.md` 拥有。
- 最小反馈信息与公开资料保护沿 [REQ-ENTRY-PREVIEW-FEEDBACK-003](#req-entry-preview-feedback-003)。

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

### 6.1.1 反馈要求与验收

以下将本节既有反馈义务分别赋予稳定身份，供专业设计和验证准确承接；适用阶段、行为和原排除范围不变。

<a id="req-entry-preview-feedback-001"></a>
#### REQ-ENTRY-PREVIEW-FEEDBACK-001：反馈入口与提交结果必须真实

Trace ID: O7-REQ-bafbce55-b1f3-414c-98e8-2cb5e70e6d1e

- 功能点：正式反馈入口的可用性、提交结果和有依据的恢复说明。
- 适用范围：受控 preview 当前声明的候选、primary mode 和正式反馈入口。
- 要求：入口如实呈现当前可用性；可提交时玩家能够确认提交结果，入口不可用或提交未完成不得计作成功。恢复下一步仅限当前专业 authority 与已验证入口支持的操作；不声称尚未实现的禁用 CTA 修复引导。该承诺不包含公开反馈账本、必达交付、保存期限、审核或调解、人工支持或逐条答复。
- 系统设计状态：已覆盖
- 系统设计依据：[支持入口与结果](../../readme/governance/player-support-and-service-lifecycle.design.md#support-entry-results)承接实际路径、结果文案和说明的采用；Launcher 的传输、门控及回落沿原反馈专业合同。
- 实现状态：部分实现
- 实现与验证依据：[native 反馈窗口](../../../crates/oasis7_client_launcher/src/feedback_window.rs)、[native 提交与本地回落](../../../crates/oasis7_client_launcher/src/feedback_entry.rs)、[Web 反馈窗口](../../../crates/oasis7_client_launcher/src/feedback_window_web.rs)已有各自门控与结果消费；这是源码审读，不代表同候选 UI 或网络验收。
- 核对日期与基线：2026-11-03；源码 `d21019414c46d7e8ba61ab2c59b1f217b144ba88`；未执行运行、浏览器或真实反馈提交。
- 剩余范围：支持正文的实际采用、当前声明入口的可用性、提交结果与恢复组合仍需按候选验证；本地文件和远端受理不等于人工已经接收处理。
- 验收：[AC-ENTRY-PREVIEW-FEEDBACK-001](#ac-entry-preview-feedback-001)。

<a id="ac-entry-preview-feedback-001"></a>
#### AC-ENTRY-PREVIEW-FEEDBACK-001：反馈结果不会互相代签

- 覆盖要求：[REQ-ENTRY-PREVIEW-FEEDBACK-001](#req-entry-preview-feedback-001)。
- 给定：同一候选声明的反馈入口，以及其实际支持的可用、拒绝、远端结果或本地保存情形。
- 当：玩家查看入口并提交，或提交未完成、结果尚不明确。
- 则：说明与当前路径的权威结果一致；不可用、未完成不报成功；仅本地保存不写成远端送达；恢复建议确实受支持，不暗示人工接单或世界行动已完成。
- 方法：[入口与结果核对](../../readme/governance/player-support-and-service-lifecycle.manual.md#support-entry-check)分别记录来源审读与实际操作；桌面推演不能代签运行。

<a id="req-entry-preview-feedback-002"></a>
#### REQ-ENTRY-PREVIEW-FEEDBACK-002：有效反馈有处置和下一步

Trace ID: O7-REQ-9bd3819f-e0fa-489c-9f36-6dd2f415f9ef

- 功能点：受控 preview 的反馈分流、处置与可复现阻断重验。
- 适用范围：本节 LP-3、LP-4 中的有效反馈信号，沿用既有运营与 QA 职责。
- 要求：反馈按 `Blocking / Opportunity / Idea` 分类，并记录 owner、处置结论与下一步。可复现的 blocking 信号必须使同一候选重新进入受影响专业域与 QA gate，旧 gate pass 不能代签；分类和处置记录不形成逐条人工答复或响应时限承诺。
- 系统设计状态：已覆盖
- 系统设计依据：[分流、处置与复核](../../readme/governance/player-support-and-service-lifecycle.design.md#support-triage-review)承接真实分类、职责、处置理由、下一步和新证据复核；不新增自动工单协议或第六种 Issue 类型。
- 实现状态：未核实
- 实现与验证依据：已有[渠道升级方法](../../readme/governance/readme-moltbook-liveops-runbook.md#8-升级矩阵)和专业验证入口；本轮未取得当前 preview 的实际反馈、处置、重验与结果告知记录，不能由方法存在推定闭环已执行。
- 核对日期与基线：2026-11-03；文档与源码 `d21019414c46d7e8ba61ab2c59b1f217b144ba88`；仅静态核对，不恢复历史邀请或轮次。
- 剩余范围：当前候选的有效信号分流、owner 接收、受影响 gate 重验及实际处置结果须分别留证；其他渠道不能由单一渠道记录代签。
- 验收：[AC-ENTRY-PREVIEW-FEEDBACK-002](#ac-entry-preview-feedback-002)。

<a id="ac-entry-preview-feedback-002"></a>
#### AC-ENTRY-PREVIEW-FEEDBACK-002：阻断反馈进入同候选复核

- 覆盖要求：[REQ-ENTRY-PREVIEW-FEEDBACK-002](#req-entry-preview-feedback-002)。
- 给定：同候选的有效阻断、体验摩擦或建议，以及可补充的新证据。
- 当：运营与专业 owner 分流、确认影响或复核原结论。
- 则：每条有效信号能定位分类、owner、处置理由与下一步；可复现 blocker 回到受影响专业域和当前候选 QA gate；处置或关闭记录不自动证明代码已修复、世界已恢复或公开阶段升级。
- 方法：[分流与复核核对](../../readme/governance/player-support-and-service-lifecycle.manual.md#support-triage-check)保留桌面案例与真实处置的不同结果。

<a id="req-entry-preview-feedback-003"></a>
#### REQ-ENTRY-PREVIEW-FEEDBACK-003：反馈只索取最小必要信息

Trace ID: O7-REQ-eb290f7e-b49e-42dc-ba88-016ed9c87808

- 功能点：反馈所需信息与公开资料的保护边界。
- 适用范围：受控 preview 收集、分流和公开说明所用的反馈与证据。
- 要求：只索取复现和分流所需的最小信息；不得把账号、私密渠道内容、敏感配置/凭据或其他不必要的个人信息写入公开证据。支持材料应依据实际渠道说明可见与保留边界，不由自动诊断清洗推定用户手写内容安全，也不承诺当前未提供的私密受理或删除能力。
- 系统设计状态：已覆盖
- 系统设计依据：[支持数据边界](../../readme/governance/player-support-and-service-lifecycle.design.md#support-data-boundaries)承接最小内容、各路径数据流、公开告知与误传处置；本地诊断和 DistFS 数据合同仍由原专业 authority 拥有。
- 实现状态：部分实现
- 实现与验证依据：[缺陷模板](../../../.github/ISSUE_TEMPLATE/defect.yml)提示不上传凭据/敏感日志；[本地反馈处理](../../../crates/oasis7_client_launcher/src/feedback_entry.rs)包含 allowlist 及有界诊断；[反馈公开读取](../../../crates/oasis7_distfs/src/feedback.rs)保留追加历史。上述源码与模板存在不证明所有输入均已脱敏或误传资料可删除。
- 核对日期与基线：2026-11-03；源码 `d21019414c46d7e8ba61ab2c59b1f217b144ba88`；未执行隐私、复制或真实误传处置验证。
- 剩余范围：本次告知的实际采用、各候选公开内容检查及误传处置仍待验证；统一保留/删除政策与未提供的私密入口不由本条虚构为已具备。
- 验收：[AC-ENTRY-PREVIEW-FEEDBACK-003](#ac-entry-preview-feedback-003)。

<a id="ac-entry-preview-feedback-003"></a>
#### AC-ENTRY-PREVIEW-FEEDBACK-003：公开资料保持最小且不误述删除能力

- 覆盖要求：[REQ-ENTRY-PREVIEW-FEEDBACK-003](#req-entry-preview-feedback-003)。
- 给定：现有公开或本地反馈路径、普通复现材料及虚构的敏感信息反例。
- 当：准备提交、提取运营摘要、公开说明或处理误传。
- 则：只请求必要复现信息，公开材料使用脱敏摘要和可公开定位；说明符合实际保存/复制与访问范围；未执行或无法证明的隐藏、删除、全网撤回及私密受理不得标记成功。
- 方法：[数据与误传核对](../../readme/governance/player-support-and-service-lifecycle.manual.md#support-data-check)使用虚构输入做静态/桌面检查，真实数据操作另按权限和渠道能力执行。

### 6.1.2 发行材料与项目支持边界

以下要求承接候选实际声明的发行范围，以及项目可管理的支持和社区表面。它们不选择新的发行国家、商店、收费服务或私密收件入口；已有预览的传输、世界权威和公开状态保持各自原合同。

<a id="req-entry-release-materials-001"></a>
#### REQ-ENTRY-RELEASE-MATERIALS-001：发行材料必须对应实际渠道和受众

Trace ID: O7-REQ-a0b94d0f-456c-4702-8933-2232fdf0a544

- 功能点：候选的渠道、地区、受众与发行材料适用性。
- 适用范围：准备对外提供的候选及其实际选定入口；内部设计审读不等于选择或开放新渠道。
- 要求：发布说明应能定位实际渠道、支持平台/语言、可进入范围、受众限制、品牌和素材权利，以及适用的内容分级、使用协议和数据告知材料。未选择的渠道不能默认为已支持；无法确认受众、地区或适用材料时，受影响发布范围保持待核，不用仓库许可证、免费进入目标或其他渠道结论代签。内容或数据使用变化时重核对应材料及公开 claim。
- 系统设计状态：部分覆盖
- 系统设计依据：[发行材料与适用性](../../readme/governance/player-support-and-service-lifecycle.design.md#support-release-materials)承接内容输入、责任和缺项判定；实际商店/地区及相关专业执行尚须分别核实。
- 实现状态：未核实
- 实现与验证依据：已有 README、官网和候选发行要求；尚无本条完整范围的同候选渠道材料和专业核定记录。仓库内容可访问不证明全球发行许可。
- 核对日期与基线：2026-11-04；文档/源码 `f4f8e1bb1be6cb9746ea767af3bdf71204e347e3`；仅设计与源审读。
- 剩余范围：具体候选的渠道/地区/受众选择、适用材料实际准备和核定、当前对外入口采用与验收。
- 验收：[AC-ENTRY-RELEASE-MATERIALS-001](#ac-entry-release-materials-001)。

<a id="ac-entry-release-materials-001"></a>
#### AC-ENTRY-RELEASE-MATERIALS-001：材料缺项会限制对应发布范围

Trace refs: O7-REQ-a0b94d0f-456c-4702-8933-2232fdf0a544；承接[REQ-ENTRY-RELEASE-MATERIALS-001](#req-entry-release-materials-001)。

- 给定同候选的实际渠道/受众声明、素材和数据用途；逐项核对有依据的适用、不适用和未知。
- 缺少适用材料、没有真实支持证据或素材权利未核实的范围不能宣称已就绪。一个渠道的齐备结果不能扩大另一渠道或地区；后续修订保留原事实与新判定。
- 方法：[发行材料核对](../../readme/governance/player-support-and-service-lifecycle.manual.md#verify-release-materials)；静态案例不等于法务、平台审核或真实发布通过。

<a id="req-entry-community-001"></a>
#### REQ-ENTRY-COMMUNITY-001：项目社区处置应有行为依据和复核路径

Trace ID: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79

- 功能点：项目可管理的公开社区、反馈讨论及相关滥用报告的内容处置。
- 适用范围：已确认由项目管理、且管理者实际具备权限的讨论或渠道；第三方社区只能按平台规则转交，不能虚构跨平台处罚权。
- 要求：行为规则、受理方式、处置理由和复核方式可查；区分规则内竞争损失、正常批评、重复或偏题内容、骚扰/威胁、欺诈冒充、垃圾信息与敏感信息公开。依据实际证据和权限选择最小适当动作，必要紧急保护可以先执行并补充不泄露敏感内容的说明。原处置者有利益冲突时由其他有权限人员复核；尚无可用复核者时如实保留待复核状态。社区内容处置不改变玩家世界资产、控制权、历史或合同结果。
- 系统设计状态：部分覆盖
- 系统设计依据：[社区行为、处置与复核](../../readme/governance/player-support-and-service-lifecycle.design.md#support-community-rules)定义内容与人工责任；实际平台权限、处置和复核能力须逐渠道验证。
- 实现状态：未核实
- 实现与验证依据：已有公开 Issue 和渠道回流方法；未取得各渠道已采用本规则、具备相应权限或完成真实处置/复核的记录。
- 核对日期与基线：2026-11-04；固定文档/源码 `f4f8e1bb1be6cb9746ea767af3bdf71204e347e3`；仅静态审读。
- 剩余范围：公开规则采用、实际管理和复核人员、可用权限、敏感报告的安全受理能力及真实案例验证；不承诺逐条答复或统一响应时限。
- 验收：[AC-ENTRY-COMMUNITY-001](#ac-entry-community-001)。

<a id="ac-entry-community-001"></a>
#### AC-ENTRY-COMMUNITY-001：竞争、批评和滥用不会混为同类

Trace refs: O7-REQ-cd283368-58dd-4538-93a8-7dae3cb2de79；承接[REQ-ENTRY-COMMUNITY-001](#req-entry-community-001)。

- 用同一公开行为规则比较合法竞争损失、强烈但无攻击的批评、重复刷屏、具体骚扰、敏感误传和证据不足的报告；按实际内容和风险分别处理。
- 结果能定位依据、实际动作范围与复核下一步；隐藏、删除、锁定或限制参与按平台真实语义说明，未执行不报成功。世界结果和他方私密内容不因讨论处置被改写或公开。
- 方法：[社区处置核对](../../readme/governance/player-support-and-service-lifecycle.manual.md#verify-community-disposition)；虚构案例不代签真实权限和操作。

<a id="req-entry-support-data-001"></a>
#### REQ-ENTRY-SUPPORT-DATA-001：支持资料按用途和控制范围保留

Trace ID: O7-REQ-96d24911-2209-4606-999e-545e4cd10fe9

- 功能点：项目为反馈、支持、社区处置或事故核查持有的资料及相关数据请求。
- 适用范围：现有渠道产生、且项目实际控制的运营副本和摘要；玩家本地数据、第三方平台、公开复制账本与世界权威历史分别遵守各自主责合同。
- 要求：收集前说明用途、必要范围、访问者、实际公开性、保留或复核规则及可用请求方式；不因新增本条自动扩展诊断或收集个人资料。项目可控副本采用明确的最小保留和到期处置规则，不以“可能有用”无限保存；未结争议或确有适用保留依据的材料注明理由、最小范围和复核点。请求先最小化确认请求者与材料关联，区分已删除、仅隐藏/撤回当前展示、第三方待处理、不可控副本及保留例外；没有执行或核实能力不得承诺全网删除。敏感误传优先停止扩散并保护受影响访问凭据，不能把追加 tombstone 当历史擦除。
- 系统设计状态：部分覆盖
- 系统设计依据：[保留与数据请求](../../readme/governance/player-support-and-service-lifecycle.design.md#support-retention-requests)承接人工内容政策及分流；各技术数据源的访问、保留、复制和实际删除由原专业 authority 执行。
- 实现状态：未核实
- 实现与验证依据：现有 Launcher 本地反馈、GitHub 和 DistFS 路径的能力不同；本条没有新增私密入口、自动删除或全网擦除实现。当前技术边界见[原反馈数据合同](../../readme/governance/player-support-and-service-lifecycle.design.md#support-data-boundaries)。
- 核对日期与基线：2026-11-04；固定文档/源码 `f4f8e1bb1be6cb9746ea767af3bdf71204e347e3`；未操作真实个人资料或执行删除。
- 剩余范围：当前持有资料、访问权限、政策告知采用、各渠道实际请求与处置验证；未知平台/地区的法定义务须按实际范围核定，不能由本条代签。
- 验收：[AC-ENTRY-SUPPORT-DATA-001](#ac-entry-support-data-001)。

<a id="ac-entry-support-data-001"></a>
#### AC-ENTRY-SUPPORT-DATA-001：保留与删除说明对应实际副本

Trace refs: O7-REQ-96d24911-2209-4606-999e-545e4cd10fe9；承接[REQ-ENTRY-SUPPORT-DATA-001](#req-entry-support-data-001)。

- 给定项目可控临时副本、公开 Issue、玩家本地包、只追加账本和受限争议材料，检查正常到期、误传、删除请求、权限不足和保留例外。
- 每类资料能定位目的、控制者、实际可做动作及下一复核点；已删除对象与无法控制的历史/副本分别说明，处置记录不继续复制原秘密；请求不会变为取消世界义务或删除他人数据的授权。
- 方法：[保留与数据请求核对](../../readme/governance/player-support-and-service-lifecycle.manual.md#verify-retention-requests)；桌面演练与真实删除、平台回复分别判断。

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
| [REQ-ENTRY-PERFORMANCE-001](#req-entry-performance-001) / [AC-ENTRY-PERFORMANCE-001](#ac-entry-performance-001) | `viewer_engineer` / `qa_engineer` | [显示投影与调度](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#viewer-render-projection) | 同候选多负载/多屏交互、数据新鲜度和转换次数；目标设计，运行验收待实施 | `test_tier_required`；真实浏览器 S6 |
| [REQ-ENTRY-PERFORMANCE-002](#req-entry-performance-002) / [AC-ENTRY-PERFORMANCE-002](#ac-entry-performance-002) | `viewer_engineer` / `qa_engineer` | [资源交付与恢复](../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#viewer-payload-delivery) | 冷/热加载、缓存淘汰、版本切换、损坏/离线和实部署压缩；目标设计，运行验收待实施 | `test_tier_required`；真实浏览器 S6 |
| [REQ-ENTRY-PREVIEW-FEEDBACK-001](#req-entry-preview-feedback-001) / [AC-ENTRY-PREVIEW-FEEDBACK-001](#ac-entry-preview-feedback-001) | `liveops_community` / `viewer_engineer` / `qa_engineer` | [支持入口与结果](../../readme/governance/player-support-and-service-lifecycle.design.md#support-entry-results) | [入口核对方法](../../readme/governance/player-support-and-service-lifecycle.manual.md#support-entry-check)；静态、桌面与实际提交分别留证，不由本地保存或受理推定人工处理 | `test_tier_required`；实际入口按原 S6/Launcher 方法 |
| [REQ-ENTRY-PREVIEW-FEEDBACK-002](#req-entry-preview-feedback-002) / [AC-ENTRY-PREVIEW-FEEDBACK-002](#ac-entry-preview-feedback-002) | `liveops_community` / `qa_engineer` / `producer_system_designer` | [分流与复核](../../readme/governance/player-support-and-service-lifecycle.design.md#support-triage-review) | [分流核对方法](../../readme/governance/player-support-and-service-lifecycle.manual.md#support-triage-check)；实际 owner、处置和同候选重验记录待取得 | `test_tier_required`；人工运营核对，受影响专业域沿原 test tier |
| [REQ-ENTRY-PREVIEW-FEEDBACK-003](#req-entry-preview-feedback-003) / [AC-ENTRY-PREVIEW-FEEDBACK-003](#ac-entry-preview-feedback-003) | `liveops_community` / `qa_engineer` / 对应数据与安全 owner | [支持数据边界](../../readme/governance/player-support-and-service-lifecycle.design.md#support-data-boundaries) | [数据与误传方法](../../readme/governance/player-support-and-service-lifecycle.manual.md#support-data-check)；虚构材料审读不代签真实隐私、复制或删除结果 | `test_tier_required`；真实数据操作另按授权与原方法 |
| [REQ-ENTRY-RELEASE-MATERIALS-001](#req-entry-release-materials-001) / [AC-ENTRY-RELEASE-MATERIALS-001](#ac-entry-release-materials-001) | `producer_system_designer` / `liveops_community` / `qa_engineer` 及实际平台或数据 owner | [支持与生命周期专业条款](../../readme/governance/player-support-and-service-lifecycle.design.md#support-release-materials) | [核对方法](../../readme/governance/player-support-and-service-lifecycle.manual.md#verify-release-materials)；当前仅设计与静态核对，实际渠道、权限和执行分别验证 | `test_tier_required`；实际技术执行沿原专业方法 |
| [REQ-ENTRY-COMMUNITY-001](#req-entry-community-001) / [AC-ENTRY-COMMUNITY-001](#ac-entry-community-001) | `producer_system_designer` / `liveops_community` / `qa_engineer` 及实际平台或数据 owner | [支持与生命周期专业条款](../../readme/governance/player-support-and-service-lifecycle.design.md#support-community-rules) | [核对方法](../../readme/governance/player-support-and-service-lifecycle.manual.md#verify-community-disposition)；当前仅设计与静态核对，实际渠道、权限和执行分别验证 | `test_tier_required`；实际技术执行沿原专业方法 |
| [REQ-ENTRY-SUPPORT-DATA-001](#req-entry-support-data-001) / [AC-ENTRY-SUPPORT-DATA-001](#ac-entry-support-data-001) | `producer_system_designer` / `liveops_community` / `qa_engineer` 及实际平台或数据 owner | [支持与生命周期专业条款](../../readme/governance/player-support-and-service-lifecycle.design.md#support-retention-requests) | [核对方法](../../readme/governance/player-support-and-service-lifecycle.manual.md#verify-retention-requests)；当前仅设计与静态核对，实际渠道、权限和执行分别验证 | `test_tier_required`；实际技术执行沿原专业方法 |
