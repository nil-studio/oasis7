# 客户端启动器区块链浏览器设计（当前 authority）

> 对应需求: `doc/world-simulator/launcher/game-client-launcher-blockchain-explorer.prd.md`
> 历史迁移、验收与 task 状态：GitHub task issue evidence。
> **退役与承接边界（[#7321](https://github.com/nil-studio/oasis7/issues/7321)）**：原 `oasis7_client_launcher` 同源 `egui` explorer 窗口已决定退役下线（代码清理由 [#7325](https://github.com/nil-studio/oasis7/issues/7325)、[#7326](https://github.com/nil-studio/oasis7/issues/7326) 执行）。本文保留 `oasis7_web_launcher` `/api/chain/explorer/*` 只读代理边界及查询参数/结果一致性条款供活跃消费者使用；原 `egui` 视图层与逐帧调度说明仅保留锚点与历史审读上下文。

- 后端专业设计：[链 HTTP 提交与 Explorer P0 后端合同](../../p2p/blockchain/chain-http-submit-explorer-contract.design.md)。Node 身份、批次处置、状态合并、锁序和索引保留由该专题承接。

## 表现结构

1. **只读代理承载层**：`oasis7_web_launcher` 控制面代理已有 `/api/chain/explorer/*` 只读请求（原同源 `egui` explorer 表面按 #7321 退役）。
2. **概览与命令层**：顶部保留刷新、重置当前筛选、当前 tab 与请求/链可用状态；概览以已有 height、identity、hash 与状态计数形成可扫描分组，而不是把日志行当作主界面。
3. **七视图层**：`Blocks`、`Txs`、`Contracts`、`Mempool` 使用已有列表到详情的选择路径；`Search` 显示查询和命中说明；`Address`、`Assets` 以 summary-first 卡片与次级列表呈现既有返回字段。
4. **状态层**：`loading`、`empty`、`not_ready` 与结构化 `error_code + error` 留在当前 tab；筛选、分页、清空与选择严格复用既有请求参数和状态机，不在前端重算链结果。

## 控制与信息边界

- explorer 只显示当前接口可返回的公共账本字段；不得在展示层暴露私钥、凭据或本地敏感配置。
- 该表面只能编排已有查询和结果，不得新增交易、钱包、validator、结算或世界状态控制能力。
- “mainnet-grade”只描述历史信息架构目标；设计不主张 live mainnet、公开可用、network readiness、无重置、全 archive 或最终性。
- 窄宽度可以把双区布局压为纵向，但不能回退为无状态说明的日志式长文本；任何实际视觉/交互改动须由 game_visual_interaction_designer 定义验收并按 `testing-manual.md` S6 取证。

## 代码与协议接点

- 活跃控制面代理：`oasis7_launcher_backend` 中的 `/api/chain/explorer/*`，代理 runtime 的现有 `/v1/chain/explorer/*` 查询（原 `crates/oasis7_client_launcher/src/explorer_window*` 按 #7321 / #7326 退役删除）。
- 本设计记录现行只读代理边界，不授权新增 endpoint、字段、轮询频率、索引保留策略或 DOM/UI 改动。

## 查询一致性补充（目标设计）

本次采用小型兼容编辑卡：只补充现有查询的可见状态、参数传递和结果一致性；原有七视图结构及控制边界继续有效。[Issue #4450](https://github.com/eng-cc/oasis7/issues/4450#issuecomment-6126251006) 已以 `not_planned` 停止独立跟踪，仅作历史来源；H1～H4 原义务仍有效，运行时实现与行为验收仍待落实，后续结果记在对应实现 PR，不表示当前客户端或后端已经通过验收。

- 受影响条款：表现结构中的状态层、代码与协议接点，以及下列三个查询条款。
- 外部依赖：`runtime_engineer` 拥有后端事实；`viewer_engineer` 承接 native/Web 参数与呈现；适用 QA 负责同候选接口及表面验证。
- 设计取舍与剩余风险：前端按响应呈现，不以本地状态推断补偿后台竞态。后台保证未实现前，可见结果仍可能不一致；验证必须核对实际请求与权威返回。本文不另立提交、存储、迁移或容量方案，因此不扩写无关技术段落。
- 实际 candidate、命令、结果和产物回到实现 PR 与验证记录；本次文档检查仅证明结构和引用。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [已确认事实与观察超时](game-client-launcher-blockchain-explorer.prd.md#explorer-query-lifecycle) | 按真实状态及区块关联呈现，重复登记不造成列表重复，超时可被有效确认纠正 | [可见生命周期](game-client-launcher-blockchain-explorer.design.md#des-explorer-query-lifecycle) | runtime 状态合并与有限索引；viewer/QA | 不由前端建立确认或推断结算、最终性；产品 N/A；reason: 后端状态与区块关联的忠实展示，不新增状态 authority，未查到独立 Explorer 玩家叶级 AC；scope: 不授予交易、钱包、External Agent 世界查询或 finality 能力，accepted/confirmed/timeout 不代签世界结果。 |
| [独立查询与一致展示](game-client-launcher-blockchain-explorer.prd.md#explorer-query-independent) | 直接查询无其他页面前置，空态、未就绪与结构化错误可区分 | [独立查询接点](game-client-launcher-blockchain-explorer.design.md#des-explorer-query-independent) | runtime P0 同步入口；既有 native/Web 只读代理 | 不建立前端同步服务、保留策略或数值 SLA；产品 N/A；reason: P0-only 读取与错误透传，不新增世界读取权限，未查到独立 Explorer 玩家叶级 AC；scope: 不授予交易、钱包、External Agent 世界查询或 finality 能力，accepted/confirmed/timeout 不代签世界结果。 |
| [组合过滤与分页](game-client-launcher-blockchain-explorer.prd.md#explorer-query-filters) | 全部已填写参数保留，结果、计数、排序、分页与回显同源 | [条件与结果传递](game-client-launcher-blockchain-explorer.design.md#des-explorer-query-filters) | runtime AND 求值；native/Web 查询构造 | 不新增参数别名、字段优先级或本地结果重算；产品 N/A；reason: 已有 HTTP 参数 AND 与分页传递，不新增交易执行，未查到独立 Explorer 玩家叶级 AC；scope: 不授予交易、钱包、External Agent 世界查询或 finality 能力，accepted/confirmed/timeout 不代签世界结果。 |
| [刷新与详情连续性](game-client-launcher-blockchain-explorer.prd.md#explorer-client-refresh-continuity) | 已加载空结果不重新安排非活动列表，当前查询实际推进 | [E1 空态](game-client-launcher-blockchain-explorer.design.md#des-explorer-e1-empty) | Launcher 调度/响应消费；QA | 产品 N/A；reason: 空列表加载历史与待办调度，不新增查询能力，未查到独立 Explorer 玩家叶级 AC；scope: 不授予交易、钱包、External Agent 世界查询或 finality 能力，accepted/confirmed/timeout 不代签世界结果。 |
| [刷新与详情连续性](game-client-launcher-blockchain-explorer.prd.md#explorer-client-refresh-continuity) | overview 到期或完成不吞掉当前 tab 待办 | [E2 慢概览](game-client-launcher-blockchain-explorer.design.md#des-explorer-e2-overview) | Launcher 调度/响应消费；QA | 产品 N/A；reason: overview 与当前 tab 待办推进，不新增轮询 SLA，未查到独立 Explorer 玩家叶级 AC；scope: 不授予交易、钱包、External Agent 世界查询或 finality 能力，accepted/confirmed/timeout 不代签世界结果。 |
| [刷新与详情连续性](game-client-launcher-blockchain-explorer.prd.md#explorer-client-refresh-continuity) | 周期列表缺少选中对象不等于权威删除 | [E3 详情](game-client-launcher-blockchain-explorer.design.md#des-explorer-e3-detail) | Launcher 调度/响应消费；QA | 产品 N/A；reason: 周期列表与独立详情连续性，不新增账本保留承诺，未查到独立 Explorer 玩家叶级 AC；scope: 不授予交易、钱包、External Agent 世界查询或 finality 能力，accepted/confirmed/timeout 不代签世界结果。 |

<a id="des-explorer-query-lifecycle"></a>
### 可见生命周期

同一 action 的状态和区块关联来自后端响应；前端不得把重新获取 accepted 元数据解释为新的交易，也不得重复追加同一返回条目。迟到接受登记、重复读取及已保存索引重载后，已确认条目仍按返回的确认和块关联显示。

`timeout` 沿用观察超时含义；之后收到有效 `confirmed` 时更新对应条目。确定 `failed` 按真实失败原因展示，不从等待时间合成失败；未知结果不触发自动新建动作重提。后端仍返回矛盾事实时交由其专业合同定位，界面不自行选取“更成功”的状态遮蔽问题。

<a id="des-explorer-query-independent"></a>
### 独立查询接点

native/Web 和直接 API 消费者均可通过既有 `tx`、`txs`、`blocks`、`search` 发起独立读取。既有 overview 展示可继续存在，但不得成为这些查询正确性的必要前序步骤；Web 代理原样传递结果和结构化失败。

一次响应以[后端读取边界](../../p2p/blockchain/chain-http-submit-explorer-contract.design.md#des-http-h3-independent-query)为准；不同时间的响应可能包含不同提交，不由前端拼成虚构的原子快照。请求中的 loading、正常空结果、not_ready 和 error 继续各有状态；后端同步失败不可在代理或客户端降格成成功空列表。

<a id="des-explorer-query-filters"></a>
### 条件与结果传递

Txs 构造请求时同时保留有效的 `action_id`、`account_id`、`status` 及既有 cursor/limit。账户控件继续映射 HTTP `account_id`；填写 action ID 不删除其他参数，不引入 `account` 别名。清空或重新筛选按现有状态机重置相关选择和分页。

条件回显使用本次实际请求；列表、`total`、游标与排序使用该请求的后端结果。action 命中但账户或状态不匹配时呈现 `items=[]`、`total=0` 的正常空态，不保留上一次命中的详情冒充本次结果。全部匹配及仅 action ID 的查询仍可正常选中对应交易。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确 test/manual source、scenario/layer 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [已确认事实与观察超时](game-client-launcher-blockchain-explorer.prd.md#explorer-query-lifecycle) | [可见生命周期](game-client-launcher-blockchain-explorer.design.md#des-explorer-query-lifecycle) | 确认、块关联及唯一条目按后端事实呈现；迟到确认纠正观察超时 | [测试手册](../../../testing-manual.md) S6 Explorer/Transfer 条目；同候选隔离后端构造已确认后迟到登记、超时后确认及已保存索引重载，核对 native/Web 实际响应、可见状态和条目，planned；后端双 store barrier 单独验证 | #4450 H2 历史来源与后续实现 PR 的接口/表面证据 | 本次未运行；截图不证明后台幂等、持久化或执行最终性 |
| [独立查询与一致展示](game-client-launcher-blockchain-explorer.prd.md#explorer-query-independent) | [独立查询接点](game-client-launcher-blockchain-explorer.design.md#des-explorer-query-independent) | P0 独立调用、结构化失败透传、空态与错误有别 | [测试手册](../../../testing-manual.md) S6 Explorer/Transfer 条目；同候选禁止 overview/status/history 后，核四类查询的 native/Web 代理请求与结果，并注入同步失败核错误，planned | #4450 H3 历史来源与后续实现 PR 的请求/响应及表面证据 | 本次未运行；默认页面顺序不证明 P0-only，无数值 SLA |
| [组合过滤与分页](game-client-launcher-blockchain-explorer.prd.md#explorer-query-filters) | [条件与结果传递](game-client-launcher-blockchain-explorer.design.md#des-explorer-query-filters) | 组合参数不丢失，空结果、计数、排序、分页和回显一致 | [测试手册](../../../testing-manual.md) S6 Explorer/Transfer 条目；同候选 native/Web 分别覆盖账户错、状态错、全匹配、仅 action ID、清空和分页，采集实际 query 与返回并核呈现，planned | #4450 H4 历史来源与后续实现 PR 的参数矩阵和表面证据 | 本次未运行；单 status 测试或前端 fixture 不证明真实组合查询 |
| [刷新与详情连续性](game-client-launcher-blockchain-explorer.prd.md#explorer-client-refresh-continuity) | [E1 空态](game-client-launcher-blockchain-explorer.design.md#des-explorer-e1-empty) | 已加载空结果不重新安排非活动列表，当前查询实际推进 | [测试手册](../../../testing-manual.md) S6；`explorer_window.rs`、`explorer_window_view.rs` 与受控 transport；成功空 Blocks 转 Search/Address；非空 Blocks 加空 filtered Txs 转 Search/P1，native/Web；planned | 实现 PR 的完整消费链：接收→清 in-flight→下一帧→实际请求；保留正常空态和显式/周期刷新 | 本次未运行；不证明后端一致性或世界结果 |
| [刷新与详情连续性](game-client-launcher-blockchain-explorer.prd.md#explorer-client-refresh-continuity) | [E2 慢概览](game-client-launcher-blockchain-explorer.design.md#des-explorer-e2-overview) | overview 到期或完成不吞掉当前 tab 待办 | [测试手册](../../../testing-manual.md) S6；`explorer_window.rs`、`explorer_window_view.rs` 与受控 transport；非空已加载列表，连续约 1100ms overview；点击至空闲帧不足/超过一秒及快响应对照，native/Web；planned | 实现 PR 的完整消费链：有限轮次发出当前 tab 请求，不只检查 pending/last_poll | 本次未运行；不证明后端一致性或世界结果 |
| [刷新与详情连续性](game-client-launcher-blockchain-explorer.prd.md#explorer-client-refresh-continuity) | [E3 详情](game-client-launcher-blockchain-explorer.design.md#des-explorer-e3-detail) | 周期列表缺少选中对象不等于权威删除 | [测试手册](../../../testing-manual.md) S6；`explorer_window.rs`、`explorer_window_view.rs` 与受控 transport；detail→周期调度→快速非空头页；Search 页外 block、过滤外 tx、页内同对象及明确清空/换选/not_found，native/Web；planned | 实现 PR 的完整消费链：页外详情保留，页内更新；直接 detail 失败不伪造 not_found；新筛选不保留假命中 | 本次未运行；不证明后端一致性或世界结果 |

后端身份与可靠处置的 H1 不由本界面表验收；其五项独立义务和定向测试入口以[后端验证映射](../../p2p/blockchain/chain-http-submit-explorer-contract.design.md#http-validation)为准。发生实际视觉或交互代码改动时，继续按上文 S6 与适用设计角色要求取证。

<a id="des-explorer-client-refresh-continuity"></a>
## 客户端调度与详情保留

本节承接[刷新与详情连续性](game-client-launcher-blockchain-explorer.prd.md#explorer-client-refresh-continuity)，针对 [#4406 E1–E3](https://github.com/eng-cc/oasis7/issues/4406) 的客户端消费，不替代 #4450 历史来源所承接的后端一致性设计。

<a id="des-explorer-e1-empty"></a>
### 调度状态

每个查询保留“尚未加载、在途、已加载（可为空）、错误”的区分，不用列表长度代替加载历史。窗口每帧显示空态不会重新安排非活动列表；显式用户请求、筛选变化和既有当前 tab 周期刷新产生各自的待办。

<a id="des-explorer-e2-overview"></a>
### 概览与当前查询推进

概览和当前 tab 的到期状态分别维护；发送/完成 overview 不能重置或吞掉当前 tab 的待办。继续使用有界 in-flight 约束，空闲时让已等待的当前操作推进，不允许刚重新到期的 overview 永久排在其前面。可以保留现有轮询周期；关键是响应消费和下一帧的调度顺序，不是扩大周期或并发。旧查询响应只更新对应请求的查询状态，不能因 tab/筛选已改变而把结果或错误套到新输入。

<a id="des-explorer-e3-detail"></a>
### 详情状态

列表缓存与已选 detail 分开维护。周期列表响应可更新同身份的选中对象；没有命中时保留该对象的独立详情，不把 `.find(...) == None` 当作权威删除。当前对象的直接 detail 查询明确返回 not_found、用户清空或选择其他对象时，才按相应路径改变详情。失败查询仍显示真实错误，不伪造 not_found。

上述保留只针对未改变选择的周期刷新。用户主动重置或改变组合过滤时继续执行[条件与结果传递](game-client-launcher-blockchain-explorer.design.md#des-explorer-query-filters)的现有规则，避免把历史选中详情当作新筛选的命中；此设计不新定主动换页行为。

### 源码接点与独立验证

2026-10-15 固定 main `b57e30eaaa0f3eb48c273eb62363d2ac2ada8981` 静态复核的 `crates/oasis7_client_launcher/src/explorer_window_view.rs` 每帧按 `blocks/txs.is_empty()` 安排请求；`explorer_window.rs::maybe_request_explorer_panel_data` 把到期 overview 放在各 tab 待办之前；列表结果处理又按本页 `.find(...)` 重写 selected detail。三处分别对应 E1、E2、E3，不以一处修复替代另外两处。

| 独立义务 | 目标输入与完整消费链 | 需要观察的结果 |
| --- | --- | --- |
| E1 空态与调度 | 成功空 Blocks → Search/Address；非空 Blocks + 空 filtered Txs → Search/P1；接收、清 in-flight、下一帧并实际发出请求 | 当前目标请求发出；非活动空列表不逐帧重新请求；正常空态、显式刷新与当前 tab 周期刷新保留 |
| E2 慢概览 | 已加载非空列表，连续约 1100ms 成功 overview；点击到下个空闲帧分别不足/超过一秒，另保留快响应 | 当前 tab 在有限轮次实际请求并刷新；不是只验证 pending，也不靠 E1 的空列表场景代替 |
| E3 详情保留 | detail 响应 → 周期调度 → 多次快速非空头页响应；Search 页外旧 block、过滤外旧 tx、页内对象、明确换选/清空/真正 not_found | 页外详情保留、页内同对象更新、明确 detail 结果与用户选择分别生效 |

回归接点为上述 launcher 模块及其测试，可用受控查询 transport；可见变更另按 S6 核 native/Web 实际表面。本次仅补合同，未执行这些行为场景，也不证明后端查询一致性或交易最终性。

## 风险与未决

- E1 的 `is_empty()`、E2 到期 overview 优先 return、E3 列表 `.find(...)` 重写详情仍是固定 main 的现状；上文是待实现/待验目标。本轮未执行行为或浏览器回归。`activate_explorer_tab_marks_explicit_refresh_without_waiting_for_poll` 只核 pending/last_poll，不证明慢响应后的实际发送。
- 后端 `explorer_p0_store.rs::query_txs` 仍在 action_id 分支提前返回，绕过 account/status；前端参数传递不证明 AND 求值已修。只读消费此事实，不修改后端或 chain HTTP 设计。
- 旧 `explorer_p0_blocks_txs_tx_search_queries_return_expected_payloads` 先轮询 transfer/status 至 Confirmed，再查 P0；它不是禁止 overview/status/history 的独立查询验证。NoopExecutionHook 与文档 gate 不证明执行、结算或世界最终性。
- 保留普通 chain accepted、confirmed（入块观察）、timeout（观察超时）的专业语义；不外推 guarded WorldService receipt/commit、玩家可玩性或发行就绪。真实 native/Web 表面及后台竞态仍由同候选实现和 QA 分别验收。
