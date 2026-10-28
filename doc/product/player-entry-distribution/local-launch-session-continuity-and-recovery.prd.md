# 本地启动会话连续性与恢复

## 文档身份

- 所属产品模块：玩家接入与发行
- 上位产品 PRD：[`prd.md`](prd.md)
- 配对产品设计：[`local-launch-session-continuity-and-recovery.design.md`](local-launch-session-continuity-and-recovery.design.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-13
- 公开状态权威：[`README.md`](../../../README.md)
- 专业域权威：[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`Launcher 子域入口`](../../world-simulator/launcher/README.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文承载受支持本地 Launcher 路径的会话连续性与恢复产品承诺。它补充[访问模式与发行就绪](access-modes-and-release-readiness.prd.md)，不创建第三种玩家访问模式，也不定义启动器、链运行时、持久化或 Web/WASM 的实现合同。

配对产品设计的 canonical 路径为 `doc/product/player-entry-distribution/local-launch-session-continuity-and-recovery.design.md`。

## 设计适用性与生命周期闭合

- 设计判定：`paired-design`。
- 配对关系：[local-launch-session-continuity-and-recovery.design.md](local-launch-session-continuity-and-recovery.design.md) 承接本 PRD 的玩家经历、信息层级、状态反馈与恢复解释；产品真值仍由本 PRD 拥有。
- 设计适用性理由：本地 session 的状态优先级、恢复动作和 blocked 边界由同名 design 承接。
## 1. 目标

玩家通过受支持的本地 Launcher 进入已声明的 `viewer` 或 `pure_api` 路径时，能够辨认当前会话是准备、可用、阻塞、停止还是恢复中；发生中断、陈旧状态或配置问题时，能够采取真实的恢复下一步，而非把已启动进程、旧本地状态、浏览器页面或设置编辑误认为当前世界已健康、已连接或可玩。

## 1.1 体验决策

玩家根据当前模式、会话可信状态、配置阶段和 blocker，选择启动、修复输入、重试、等待或停止。各动作的阅读顺序和失败恢复见[配对产品设计](local-launch-session-continuity-and-recovery.design.md#21-状态动作结果恢复决策)；不得用局部成功替代当前 authority 确认。

## 2. 产品承诺

### 2.1 可理解的本地会话生命周期

- 正式 Launcher 表面把本地路径组织为一条可理解的生命周期：选择当前入口与模式、准备或启动、确认当前可用或 blocked 状态、停止，以及恢复或重新进入。
- Launcher 是已声明 primary mode 的交付、控制和过渡表面；本地 session、运行 lane、provider、浏览器或执行环境不会形成新的玩家模式、世界或发行等级。
- 已启动的子过程、可见页面、旧输出或局部健康状态不能单独证明玩家已进入权威世界、已获得身份/权限或已达到可玩结论。

### 2.2 中断、停止与恢复

- 会话被停止、启动失败、发现陈旧本地状态或无法恢复时，玩家能够读到当前可信状态、主要原因和适用的下一步，例如重试、修复配置、重新进入、等待或安全停止。
- 重新启动、清理或恢复本地 session 必须与权威世界结果分开表达：它们可能改变本地进程或会话上下文，不自动撤销、确认、保存或重放玩家行动与世界后果。
- 没有安全恢复路径时，surface 明确返回受支持的入口或 blocked 决策面；不得静默复用旧 authority、旧 session 或旧配置结果。

### 2.3 设置与结果边界

- Launcher 中的设置、LLM/provider 配置或 Web 表面配置是待验证的输入；编辑、保存、重载或入口可达不等于运行时已应用配置，更不等于 Agent 行为、世界行动、结算、持久化或可玩性已成功。
- 当前 authority 支持的设置结果必须真实地区分已暂存、已接受、已应用、被拒绝、blocked 或未知，并给出适用的恢复下一步。未支持的配置或凭据能力不得展示为可用。
- 诊断应帮助玩家判断本地 session 是否可恢复，但不得泄露凭据、敏感本地路径或把工程诊断提升为玩家世界事实。

### 2.4 Web 表面与跨入口连续性

- 当 Launcher 的 Web 表面不可初始化、不可轮询或发生致命错误时，玩家得到可理解的失败或恢复状态，而不是无限加载、假连接或假成功。
- native 与 Web 可以使用不同的表现、存储和恢复机制，但不能因入口不同而把配置编辑、本地 session 或诊断成功表达为不同的 primary-mode 或权威世界结果。

## 2.5 叶级产品要求与验收

<a id="req-entry-launch-001"></a>
### REQ-ENTRY-LAUNCH-001：本地 session 可用不得代签世界结果

Trace ID: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1

- 要求：Launcher 必须把入口、primary mode、本地 session、配置和当前 authority 的可用性与 Agent/world 行动结果分开表达；进程存在、页面可见、本地保存或请求受理不能单独证明可玩或世界行动成功。
- 验收：AC-ENTRY-LAUNCH-001


- 功能点：[REQ-ENTRY-LAUNCH-001](#req-entry-launch-001)；验收 [AC-ENTRY-LAUNCH-001](#ac-entry-launch-001)。
- 适用范围：受支持 native/Web Launcher 的 session、配置、primary mode 与世界结果区分；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[会话重新验证](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#session-revalidation)已承接当前进程探测、失效快照撤销、停止/失败后重验及响应代次隔离；[配置草稿/确认](../../world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.design.md#launcher-config-draft-design)承接 Restore 与已接受配置的区分；[Reload 结果](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.design.md#action-result-continuity)承接 native/Web 读回失败保草稿与成功后更新。后二者不证明 provider 鉴权、凭据注入或全部配置已应用。[本地交付合同](../../world-simulator/llm/provider-loopback-http-contract.design.md#local-provider-delivery-contract)仅补充配置输入与实际应用的区分，以及停止不撤销已接受世界请求；产物选择和进程清理本身不算作本要求覆盖，不证明provider鉴权或世界结果。
- 反馈结果局部设计依据：[反馈结果边界](../../world-simulator/launcher/game-client-launcher-feedback.design.md#feedback-result-boundary)仅承接本条本地保存/请求受理不代签世界成功的子义务；Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1。native 本地包、distributed receipt 与 Web proxy 结果不证明可玩性、配置应用或完整 session/权限；F1–F5 专业算法不作为本条全量覆盖。静态复核基线为 2026-10-15 `b57e30eaaa0f3eb48c273eb62363d2ac2ada8981` 及本次文档候选，未新增行为验收，既有状态、AC 与剩余范围不变。
- 实现状态：部分实现
- 实现与验证依据：控制面 clear_chain_session_evidence 与客户端 invalidate_chain_session_evidence/clear_chain_session_evidence 清除 identity/P2P/observability/replication；spawn 只进 Starting，状态探测后才 Ready。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_client_launcher/src/main_chain_status.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/chain_status_probe.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4696)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。 新增loopback局部关系审读基线为2026-10-15的`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`及本次文档候选；未新增行为验证。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，primary mode、配置应用/凭据、world authority 与行为结果的全部 native/Web 组合尚未核清；局部 Ready 仅为探测成功，测试定义未在本轮执行。

- 设计复核日期与基线：2026-10-15；`dffd797f` 文档候选；上述三份专业设计的五/六列表只接收各自子义务，未闭合跨模式/world authority 与凭据接线。

<a id="ac-entry-launch-001"></a>
### AC-ENTRY-LAUNCH-001：启动子步骤成功仍保持结果边界

- 覆盖要求：REQ-ENTRY-LAUNCH-001
- 场景与结果：启动、配置应用、Web 初始化或控制面局部成功但权威条件未确认时，玩家看到准备/恢复中/blocked/未知语义；世界行动保持未确认，且玩家能查看原因和适用下一步。
- 证据边界：进程、配置、Web/WASM、runtime 和 world 结果由专业 authority/QA 分别验证；产品层不定义命令、字段或错误码。

<a id="req-entry-launch-002"></a>
### REQ-ENTRY-LAUNCH-002：停止、陈旧和失败恢复必须重新验证

Trace ID: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676

- 要求：停止、重启、清理、陈旧状态或启动失败后的重新进入必须重新验证当前 authority、session、配置和 primary mode，不得静默复用旧上下文、权限或待决行动。
- 验收：AC-ENTRY-LAUNCH-002


- 功能点：[REQ-ENTRY-LAUNCH-002](#req-entry-launch-002)；验收 [AC-ENTRY-LAUNCH-002](#ac-entry-launch-002)。
- 适用范围：本地停止、重启、清理、陈旧及失败后 authority/session/配置/模式的重新验证；沿用本条原有阶段和前置条件。
- 系统设计状态：部分覆盖
- 系统设计依据：[会话重新验证](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#session-revalidation)已承接当前进程探测、失效快照撤销、停止/失败后重验及响应代次隔离；[配置草稿/确认](../../world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.design.md#launcher-config-draft-design)承接 Restore 与已接受配置的区分；[Reload 结果](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.design.md#action-result-continuity)承接 native/Web 读回失败保草稿与成功后更新。后二者不证明 provider 鉴权、凭据注入或全部配置已应用。
- 实现状态：部分实现
- 实现与验证依据：停止、失败启动、缺进程、退出清空证据，并有 session_* 回归；客户端代次拒绝 late poll/queued action 恢复旧快照。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7_client_launcher/src/main_chain_status.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4696)。本轮为固定源码审读，回归定义不等于实际通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，当前分类器仍混用共享日志最后 64 行与本轮日志，未实现当前尝试隔离；全部设置/凭据/权限重验、真实 Web 浏览器和世界恢复续跑未验证。

- 设计复核日期与基线：2026-10-15；`dffd797f` 文档候选；上述三份专业设计的五/六列表只接收各自子义务，未闭合跨模式/world authority 与凭据接线。

<a id="ac-entry-launch-002"></a>
### AC-ENTRY-LAUNCH-002：没有安全路径时保持 blocked

- 覆盖要求：REQ-ENTRY-LAUNCH-002
- 场景与结果：停止后重入、陈旧本地状态、Web 初始化失败和恢复证据冲突的样例只进入重新验证、blocked、未知/中断或安全返回；重启、刷新或自动恢复不确认、回滚、保存或重放世界结果。
- 证据边界：恢复协议、存储、时钟、Web API 和 session 分类由 Launcher/runtime/WASM/Viewer/QA authority 决定；产品层只规定禁止推断和安全下一步。

## 3. 组合验收

- LSC-1：受支持 Launcher 路径的代表性启动、blocked、停止和恢复样例能区分本地会话状态、当前 primary mode、权威世界状态与下一决策。
- LSC-2：陈旧状态、失败启动和安全停止样例不会静默恢复旧 authority 或把本地恢复表达为世界行动已经确认；没有安全路径时返回真实 blocked 或重新进入入口。
- LSC-3：设置样例能区分草稿/暂存、接受、实际应用、拒绝或未知；配置编辑、控制面可达和本地保存不代签 Agent/world 结果。
- LSC-4：Web 表面错误或不可用样例不会被表述为已连接、已启动或可玩；适用的恢复路径与当前可信 session 状态保持一致。
- LSC-5：Launcher、runtime、Viewer、QA 和公开 claim 的证据绑定同一候选与 primary mode；局部进程、浏览器或自动化 green 不能单独成立发行或可玩结论。

| 成功标准 | 专业 owner | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| LSC-1 / LSC-2 | runtime_engineer / viewer_engineer / qa_engineer | Launcher 专业 authority；`doc/world-runtime/prd.md`; `doc/testing/prd.md` | 启动、blocked、停止、恢复与安全返回的当前表面和运行事实对账 | test_tier_required |
| LSC-3 | agent_engineer / viewer_engineer / qa_engineer | Launcher 设置专业 authority；`doc/world-simulator/prd.md`; `doc/testing/prd.md` | 配置阶段、秘密安全、结果和恢复负例 | test_tier_required |
| LSC-4 | wasm_platform_engineer / viewer_engineer / qa_engineer | Launcher Web/WASM 专业 authority；`doc/testing/prd.md` | 当前 Web 初始化、错误可见性与恢复闭环 | test_tier_required |
| LSC-5 | runtime_engineer / viewer_engineer / qa_engineer / liveops_community | `README.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 同候选 primary-mode、权威结果与公开 claim 审计 | test_tier_full |

## 4. 范围与非目标

覆盖本地 Launcher session 的玩家可理解状态、恢复边界、设置结果边界及 Web 表面失败的真实表达。

不定义 CLI、进程树、signals、端口、链/共识拓扑、execution-world 路径或文件、陈旧状态分类算法、存储/恢复协议、LLM/provider schema 与凭据、WASM 时钟、轮询/API、浏览器错误签名、打包、测试命令或历史 verdict。本分册不承诺无损恢复、自动迁移、跨目录/跨设备状态保留、自动更新、普适 Web 可用性、生产 custody、公开发行或新的玩家入口模式。

## 5. 权威与冲突处理

| 产品层拥有 | 专业域拥有 |
| --- | --- |
| 本地会话的可理解生命周期、恢复选择、设置与世界结果的语义边界、跨入口不误导 | `world-simulator` Launcher 文档拥有进程、控制面、设置、Web 表面与实现合同；runtime/P2P 文档拥有权威执行、存储、恢复和治理；testing/QA 拥有验证；根 README 拥有公开 claim |

发生冲突时，本分册不得以产品承诺改写启动器或运行时合同；由 `runtime_engineer`、`viewer_engineer`、`wasm_platform_engineer`、`agent_engineer`、QA 与产品 owner 形成显式裁决。

## 6. 接口 / 数据

产品层只定义 `primary mode → 本地 session 状态 → blocker/设置阶段 → 权威结果 → 恢复下一步` 的玩家阅读语义。进程、配置、凭据、存储、时钟、Web API 和诊断字段由对应专业 authority 定义。

## 7. 里程碑

1. 形成稳定 PRD 与 design，并由模块入口可达。
2. 六组历史 Launcher 专题的产品语义归位，专业实现合同回填 current authority。
3. 仅在专业后继和活跃引用修复后删除历史源文件。

## 8. 风险

- 把本地进程、浏览器页面或旧输出误写成进入权威世界或可玩成功。
- 把配置保存或请求受理误写成 Agent/runtime 已应用或世界行动已经成立。
- 删除仍承担启动器、运行时、存储、Web/WASM 或验证真值的专业源文件。

## 9. 未决问题与假设

- 未决：具体 Launcher、Web/WASM、runtime 和 Viewer 的状态分类、恢复路径与证据组合仍由对应专业 authority 与 QA 在同一候选窗口内确认；本分册不预先宣称跨入口恢复或可玩结论。
- 假设：当本地 session 证据缺失、过期或互相冲突时，玩家看到未知/中断或 blocked，并重新验证当前 authority；本地重启、页面可见或保存成功不改变这一边界。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-ENTRY-LAUNCH-001](#req-entry-launch-001) / [AC-ENTRY-LAUNCH-001](#ac-entry-launch-001) | `producer_system_designer` | [`doc/world-simulator/prd.md`](../../world-simulator/prd.md#目标)、[会话重新验证](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#session-revalidation)及其[验证映射](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#111-验证映射表)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 待执行方法：按专业设计 §11.1 的 session 单元回归与 AC 对账；设置结果、世界结果及 Web 表现另取同候选证据，尚未执行；导航关系不等于通过证据 | `test_tier_required` |
| [REQ-ENTRY-LAUNCH-002](#req-entry-launch-002) / [AC-ENTRY-LAUNCH-002](#ac-entry-launch-002) | `producer_system_designer` | [`doc/world-simulator/prd.md`](../../world-simulator/prd.md#目标)、[会话重新验证](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#session-revalidation)及其[验证映射](../../world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md#111-验证映射表)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 待执行方法：按专业设计 §11.1 的停止/失败/退出/陈旧回归与 AC 对账；配置、授权、Web 恢复与世界结果另取证，尚未执行；导航关系不等于通过证据 | `test_tier_required` |
