# 客户端启动器区块链浏览器（当前 authority）

> 本文是启动器区块链浏览器的当前需求 authority。它收敛五组 2026-03/04 专题中已经实现或记录的只读查询、七个业务视图与状态呈现合同；本轮已完成语义回填和活跃引用修复，日期化源三件套随之退役删除，追溯使用 Git 与 GitHub task issue evidence。

- 对应设计: `doc/world-simulator/launcher/game-client-launcher-blockchain-explorer.design.md`
- 后端专业需求：[链 HTTP 提交与 Explorer 查询合同](../../p2p/blockchain/chain-http-submit-explorer-contract.prd.md)。请求身份、批次处置、索引和交易生命周期由该专题承接。
- 历史迁移、验收与 task 状态：GitHub task issue evidence。

## 目标

- launcher 的 native/Web 同源 explorer 让玩家、测试和运维人员在同一只读表面查看当前可得的链概览和查询结果，而不必依赖命令行日志。

## 范围

- 既有概览覆盖高度、节点/world 标识、最近区块或执行哈希及已有状态计数；七个业务视图为 `Blocks`、`Txs`、`Search`、`Address`、`Contracts`、`Assets`、`Mempool`。
- 本文只收敛当前 launcher 表现层、既有控制面代理与可观察结果；不定义世界规则、余额/交易计算、索引策略或 runtime 状态演化。

## 当前合同

| 表面 | 当前可观察行为 | 交互与状态 | 边界 |
| --- | --- | --- | --- |
| 概览 | 展示当前已有高度、身份、哈希与交易状态聚合 | 刷新期间、空结果、未就绪或结构化失败可见 | 只读；字段与含义以 runtime/控制面当前实现为准 |
| `Blocks` / `Txs` | 分页浏览已有区块或交易结果，并可查看选中详情 | 保留现有排序、过滤、翻页与选择语义 | 不保证完整历史或跨重启全量数据 |
| `Search` | 按已有 `height`、block/transaction hash、`action_id` 或账户标识检索并跳转 | 空查询、无命中和失败保持可解释 | 不创造新的查询类别或索引 |
| `Address` / `Contracts` / `Assets` | 查看已有账户、系统合约目录/详情、主 token 供应和持仓等查询结果 | 输入校验、分页、空态和错误态沿用既有结构化语义 | NFT 或其他能力以返回的显式 capability 状态为准 |
| `Mempool` | 查看已有 `accepted` / `pending` 交易并按现有过滤条件浏览 | 筛选、清空、分页和选中详情保持当前请求参数语义 | 不改变交易状态或提交路径 |

native 与 Web 复用同一 launcher UI 与控制面语义；Web 经既有 `/api/chain/explorer/*` 代理访问 runtime 的只读查询。任何请求中的 `loading`、无数据的 `empty`、链未就绪的 `not_ready` 及 `error_code + error` 结果必须在当前 explorer 表面可见，不能以外层日志替代。

## 接口 / 数据

- runtime 已有只读 explorer 查询位于 `/v1/chain/explorer/*`；launcher Web 代理位于 `/api/chain/explorer/*`。
- 现有查询范围包括 overview、blocks/block、txs/tx、search、address、contracts/contract、assets 与 mempool；本文不将路径可达性解释为任何对外服务可用性。
- 查询结果、排序、分页 cursor、capability 标志与错误字段仍以当前 runtime/控制面返回为真值；launcher 不在本地补造、重算或持久化这些结果。

## 查询一致性要求

以下为现有查询能力需要满足的目标合同；[Issue #4450](https://github.com/eng-cc/oasis7/issues/4450#issuecomment-6126251006) 已按文档承接决定以 `not_planned` 停止独立跟踪，仅保留历史来源；H1～H4 的原要求、实现缺口与各项行为验收继续由规范承接，实际交付记录在对应实现 PR。采纳本节不表示 H1～H4 已实现或验收通过。本文拥有可观察结果，后台状态合并、锁序和批次同步由后端专业设计定义。

<a id="explorer-query-lifecycle"></a>
### 已确认事实与观察超时

- 同一 action 已显示 `confirmed` 后，迟到或重复的接受登记不得使它退回 `accepted`、`pending` 或 `timeout`；列表不因重复登记出现重复项，已有区块关联保持一致。
- `timeout` 表示观察窗口内尚未得到确认，不能解释为确定未执行或安全重提的许可；之后取得有效确认时应显示确认结果。`failed` 必须有明确的失败处置，不能从计时超时推导。
- 已保留记录重新加载后仍应显示原有确认与区块关联；数据窗口以外的记录不由界面补造。`confirmed` 的入块观察含义不升级为结算或最终性证明。

<a id="explorer-query-independent"></a>
### 独立查询与一致展示

- `tx`、`txs`、`blocks`、`search` 应能独立反映本进程已提交且位于约定保留范围内的批次，不以用户先访问 overview、status、history 或其他客户端先刷新为前提。
- 在后端定义的读取边界内，同批交易的状态及区块关联一致；重复读取不增加重复项。并发新提交是否进入本次响应，由[后端查询合同](../../p2p/blockchain/chain-http-submit-explorer-contract.prd.md#http-h3-independent-query)界定。
- 同步或查询失败沿用结构化错误展示；正常空结果、未就绪和错误必须可区分。前端不通过本地猜测确认状态弥补后台同步缺口。

<a id="explorer-query-filters"></a>
### 组合过滤与分页

- `Txs` 中有效的 `action_id`、账户（HTTP 参数 `account_id`）和 `status` 按 AND 共同生效；action ID 命中不覆盖账户或状态条件。各单条件的已有解释保持不变。
- action ID 命中但账户或状态不匹配时显示空结果，且 `items=[]`、`total=0`；全部匹配或仅填写 action ID 时保留正确返回。
- 回显条件、结果、计数、排序及分页基于同一条件集。native/Web 完整保留组合参数，清空和重新筛选不引入隐式字段优先级。

<a id="explorer-client-refresh-continuity"></a>
## 客户端刷新与详情连续性

以下补充 [#4406 Explorer E1–E3](https://github.com/eng-cc/oasis7/issues/4406) 的客户端目标，与上文后端查询一致性分别验收：

- **空结果已加载**：成功返回的空 Blocks 或 filtered Txs 是正常加载结果，不能在每帧被解释为“从未请求”。非活动空列表不得持续占用调度，使 Search、Address 或其他当前 tab 的有效请求无法发出。显式刷新、过滤变化、当前 tab 的周期刷新及真实错误恢复仍可触发新请求。
- **当前请求能被调度**：概览和当前 tab 各自的刷新需求不能互相覆盖。即使 overview 连续耗时约 1100ms，当前 tab 的待处理请求仍须在有限轮次发出并消费响应；不能只改超时、只设置 pending，或把快响应/切 tab 的通过当作慢 overview 通过。保持既有 in-flight 上限，避免逐帧请求风暴；不新增对外时延 SLA。
- **列表与详情的含义分开**：周期刷新的一页列表不包含当前选中对象，不等于该对象不存在。Search 命中的页外旧 block 或不符合当前列表过滤的旧 tx，其详情应保留；页内同对象可按新返回更新。明确换对象、清空和当前对象的真正 detail not_found 继续改变详情。此条不定义尚未确定的主动翻页选择行为，也不允许旧详情冒充新组合筛选的命中结果。

验收须走接收响应 → 清除 in-flight → 下一帧 → 实际调度 → 消费结果的完整链。E1、E2、E3 分别取证；后端通过、pending 标志或单次 reducer 调用都不能代替。具体接点见[客户端调度设计](game-client-launcher-blockchain-explorer.design.md#des-explorer-client-refresh-continuity)。

## 不作出的承诺

- 历史专题里的 “public chain” 或 “mainnet-grade” 是当时查询范围或信息架构的名称，不构成 live mainnet、网络 readiness、公开发布或对外服务承诺。
- explorer 的 receipt-like 字段、交易状态或展示结果不是结算、最终性、资产可转移性、validator 参与或任何链规则正确性的证明。
- 不承诺无重置、跨重启永久可用、全历史 archive、Merkle 证明、钱包管理、跨链、EVM 解码或新的写操作。
- 本次 authority 迁移不修改 UI、DOM、协议、轮询、查询频率、runtime、控制面或数据保留行为；将可见结果外推为玩家支持、运维服务或发布承诺均不成立。

## 里程碑

- 已完成：早期 panel、P0、P1、UX 与信息架构专题分别记录了既有只读查询、七视图与状态呈现。
- 本轮：将已记录的共同合同收敛为 stable authority，修复默认入口，并删除五组已吸收的源三件套。
- 后续：任何 API、数据保留、可用性、公开发布或玩家控制面变化，均须在独立任务中重新定界并取证。

## 风险

- 数据不可用、链未就绪、查询失败或最近窗口不足时，explorer 的结果可能为空或失败；界面必须忠实呈现状态，不能把空白或历史缓存表示为成功。
- 同一只读 UI 不能证明 runtime、共识、结算或网络性质；若文档未跟随实现更新，容易再次产生 authority 漂移。
- 历史命名只能通过 Git、PR 与实际验证记录追溯；任何将 “public chain” / “mainnet-grade” 外推为当前承诺的解释都无效。

## 验收与追溯

- 查询合同验收分别覆盖确认后迟到登记、观察超时后迟到确认、P0 独立读取，以及 native/Web 组合过滤和分页。后端的 handler 屏障、双消费者及预算用例见[系统设计验证映射](../../p2p/blockchain/chain-http-submit-explorer-contract.design.md#http-validation)；界面验收只证明实际命中的表面。
- 文档迁移验收：`./scripts/doc-governance-check.sh`、`./scripts/readme-link-check.sh`、`git diff --check`。
- 行为或可见界面变更必须重新按 `testing-manual.md` 的命中测试层执行；触达可见表面时由 game_visual_interaction_designer 给出验收，并按 S6 获取 desktop/mobile 浏览器证据。
- 五组已吸收的源三件套已删除；删除后的追溯使用 Git 与 GitHub task issue evidence。
