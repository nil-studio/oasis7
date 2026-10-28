# 链 HTTP 提交与 Explorer P0 后端合同设计

- 专题：`p2p/blockchain`；canonical repository：`eng-cc/oasis7`。
- Owner role：`runtime_engineer`；验证联审：`qa_engineer`；客户端合同联审：`viewer_engineer`。
- 配对专业需求：[链 HTTP 提交与 Explorer P0 后端合同](chain-http-submit-explorer-contract.prd.md)。
- 固定源码审读基线：`e01e29064517f4bf375767a19a09802335828249`，2026-10-12。
- 本文规定目标设计；本次文档变更不修改运行行为。[#4450 的文档承接关闭决定](https://github.com/nil-studio/oasis7/issues/4450#issuecomment-6126251006) 已停止该 Issue 跟踪；它仅为历史来源。H1–H4 目标合同仍有效，实际实现、测试及交付事实由承接工作的 PR 与真实验证记录维护，不能从关闭推断实现或验收完成。

## 1. 问题、目标与非目标

同一 Node 的 HTTP producer、异步 worker、transfer tracker 与 Explorer 索引必须对同一请求身份和提交观察达成一致。
当前静态基线存在入口各自分配 ID、迟到 accepted 覆盖确认、P0 依赖其他端点同步、action ID 查询绕过其他过滤条件四类缺口。
目标是让已接纳请求可关联处置、确认观察不回退、直接 P0 查询自洽，并让组合过滤与分页使用同一匹配集。

本设计只覆盖既有 chain HTTP submit 到本进程 Node/tracker/Explorer 的专业合同及 native/Web 查询接点。
HTTP `200 accepted` 不代表 committed；Explorer `Confirmed` 表示当前实现的入块观察，不证明结算成功、世界效果或共识最终性。
本设计不新增钱包、费用、签名协议、WorldService authority、后台轮询服务、数值新鲜度 SLA 或跨重启全历史承诺。
源码只做静态审读；下文现有测试名称表示验证入口，不表示已执行或已通过。

本轮静态复核：`a0b6ab12ad14c6c881e48cecdb4adbdb322dd30e`（2026-10-15）。下列历史源码链接保留原审读定位；再次核对 producer、Node、两个 store、P0 route 与 query 源码后，§3 所列缺口仍适用于本轮快照。未运行行为测试，未改变实现。

## 2. 上游约束与相关角色

直接上游为配对专业 PRD 的五项 `professional_acceptance`，不新增产品价值或公开服务承诺；本专题不新增独立产品 AC。
runtime/Node 拥有身份分配、接纳、批次处置和确认事实；Explorer 拥有派生索引；客户端只传递查询条件和呈现返回结果。
签名和 nonce 继续消费[资产与密码学基线](p2p-mainnet-security-governance-readiness.prd.md#资产与密码学基线)。
失败拒绝或重排继续消费[既有共识回放原则](../consensus/consensus-code-consolidation-to-oasis7-consensus.design.md#des-ccg-commit-replay)。
`client_request_id` 继续遵守[理想交易字段合同](../token/mainchain-token-ideal-transaction.design.md#phase-1-field-and-validation-model)，不是 consensus dedupe。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: http-h1-identity](chain-http-submit-explorer-contract.prd.md#http-h1-identity) | 共用 Node 命名空间的 HTTP producer 分配不冲突 ID，明确其他 producer、重启和旧索引边界 | [des-http-h1-identity](#des-http-h1-identity) | runtime/Node owner；既有身份、保留窗口与恢复 authority | 不承诺全网全时唯一、客户端去重或新 epoch 协议；Product N/A；reason=进程内 action allocator 不定义玩家请求去重或世界写入权；scope=同 Node HTTP producer 的 ID、显式 producer、溢出和旧索引边界 |
| [professional_acceptance: http-h1-disposition](chain-http-submit-explorer-contract.prd.md#http-h1-disposition) | 已接纳请求在批次冲突后保留可关联处置；正常请求不连带丢失，预算一致 | [des-http-h1-disposition](#des-http-h1-disposition) | runtime/Node owner；QA 故障注入与真实 handler 测试 | 不以 accepted 代替 committed，不证明资产结算；Product N/A；reason=队列处置与预算不是链上 receipt 或资产结算；scope=已接纳请求关联、健康请求保留、原签名重排和预算单次释放 |
| [professional_acceptance: http-h2-lifecycle](chain-http-submit-explorer-contract.prd.md#http-h2-lifecycle) | tracker 与 Explorer 迟到 accepted 幂等合并，确认和块关联不回退，顺序索引唯一 | [des-http-h2-lifecycle](#des-http-h2-lifecycle) | runtime/Explorer owner；持久索引与 QA 屏障测试 | 不承诺窗口外历史；Timeout 不是永久失败终局；Product N/A；reason=派生索引合并不证明玩家交易最终性；scope=tracker 与 Explorer 的 accepted 幂等、时钟、顺序及 reload |
| [professional_acceptance: http-h3-independent-query](chain-http-submit-explorer-contract.prd.md#http-h3-independent-query) | P0 查询独立消费本进程已提交批次，单次 drain 更新两个消费者并保持锁序 | [des-http-h3-independent-query](#des-http-h3-independent-query) | runtime/Explorer owner；Node committed batch 有界窗口 | 不新增跨重启 replay 服务、数值 SLA 或最终性证明；Product N/A；reason=P0 派生读取不是 DCS 消费者可服务或 guarded WorldService 恢复；scope=本进程单 drain 双消费者、保留责任、失败重试与锁序 |
| [professional_acceptance: http-h4-filters](chain-http-submit-explorer-contract.prd.md#http-h4-filters) | action/account/status 按 AND 求值；回显、total、排序和分页一致 | [des-http-h4-filters](#des-http-h4-filters) | runtime 与 native/Web owner；现有查询 DTO | 不增加查询类别或改变单项账户匹配含义；Product N/A；reason=查询谓词与分页不新增产品世界执行义务；scope=action/account/status AND、回显和 native/Web 参数保留 |

## 3. 当前状态、目标状态与差距

基线范围为普通 chain runtime 的既有 HTTP 入口、进程内 Node 及可选本地 Explorer 索引；没有生产部署、跨节点或故障实测证据。
下面的“当前”是固定源码观察，“目标”是待实现合同，不是已修复状态。

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| ID 分配与接纳 | transfer、main-token、gameplay 各有入口计数器；Node 接收调用方 ID | Node 拥有共享分配与冲突检查 | 所有 producer 与重启边界须在实现中核定 | [transfer][src-transfer-id]、[main-token][src-main-id]、[gameplay][src-gameplay-id]、[Node 入队][src-node-submit] |
| 批次错误 | merge 冲突可在 incoming 已出队后释放预算并返回错误 | 逐请求拒绝或有界重排，健康请求可继续 | 不能只修两个入口的计数器 | [merge][src-merge]、[dequeue][src-dequeue]；runtime owner |
| 生命周期 | 两侧 accepted 登记可覆盖既有确认；Explorer 持久化该状态 | 确认优先、幂等补元数据、索引唯一 | H1 唯一 ID 不能消除该交错 | [tracker][src-tracker]、[Explorer][src-explorer-accepted] |
| P0 同步 | P0 route 不传 runtime；统一 batch ingestion 位于 tracker 同步路径 | 所有直接 P0 响应先同步再时间刷新 | 默认 UI 先 overview 不能证明 API 自洽 | [route][src-p0-route]、[同步][src-sync] |
| 查询过滤 | `query_txs` 的 action ID 分支提前返回 | 统一谓词后排序、计数、分页 | H1–H3 修复不能替代此项 | [query_txs][src-query] |

## 4. 边界与结构

| 组件 | 拥有的事实与写入责任 | 读取与限制 |
| --- | --- | --- |
| HTTP handler | 解析、现有鉴权/前置检查、提交请求与 accepted 响应 | 不自行创造 committed；不拥有独立 Node ID 空间 |
| Node submit/worker | 共享身份、接纳、队列/engine 预算、批次处置、committed batch | 不调用 tracker/Explorer 回调时持有 Node/队列锁 |
| runtime 同步入口 | 单次取得批次、两个消费者的完成/重试责任 | 不允许消费者各自 drain；不丢已取得而未完成的批次 |
| TransferTracker | transfer 生命周期及唯一顺序索引 | 不是 Node ID 分配器，也不是全部 gameplay 结果仓库 |
| Explorer store | 由提交/确认事实派生的交易与区块关联，现有有限索引持久化 | 不重算结算，不以索引声明共识或全量恢复成功 |
| native/Web launcher | 参数保留、分页与状态呈现 | 不补造确认、不以调用 overview 帮 P0 满足前置条件 |

当前 Explorer 的交易 ingestion 主要处理 `TransferMainToken`；本文不把现有 transfer 投影写成全部 action 已支持。
非 transfer 请求的结果关联由对应 runtime adapter 承接；缺少可读处置通道时，H1 的该 producer 仍未完成。
WorldService request identity、签名 nonce、客户端 `client_request_id`、Node action ID 与执行侧 recipe/job ID 各自保持原 authority，不互相替代。

## 5. 关键运行流程

<a id="des-http-h1-identity"></a>
### 5.1 共享身份与接纳

1. handler 完成现有格式、签名、账户/玩家绑定、nonce 和业务前置检查，再向同一 Node 提交。
2. 目标共享 allocator 由 Node submit 层持有；所有连接到该 Node 队列的 HTTP producer 使用同一分配入口，不再各自设置起始常量。
3. 保持既有非零和保留身份限制；分配使用检查过的递增，耗尽时拒绝新接纳，不 wrap、复用或重置。
4. admission 在同一同步边界检查 ID 与 payload hash/submitter 的绑定、队列容量和字节预算；成功后请求才由 Node 拥有。
5. 相同 ID、相同已验证请求的内部重投只关联原请求，不再次入队或重复申请预算；这不引入 HTTP 客户端幂等承诺。
6. 相同 ID、不同 payload 或 submitter 显式拒绝，保留原请求；返回 accepted 的请求取得可回读的关联身份。

其他显式传 ID 的 producer 也必须通过 Node 的绑定检查，不能通过绕过共享 HTTP allocator 获得冲突写入资格。
实现需盘点测试注入、复制保留身份和其他调用方；测试 fixture 的显式 ID 不能成为生产旁路。
共享 allocator 的最低保证是同一 Node 有效身份范围内不冲突；重启时不能用从 1 开始的进程计数器覆盖仍加载的旧索引。
重启高水位/现有隔离机制的具体接线由 runtime owner 在 §12 明确的触发点核定；本设计不虚构新的持久 schema、epoch 或全网 ID 协议。

<a id="des-http-h1-disposition"></a>
### 5.2 已接纳批次的处置

1. worker dequeue 只转移请求所有权；对应字节预留继续属于该请求，不能因离开入口 Vec 就视作已消费完成。
2. 首选在 admission 拒绝冲突；merge 保留防御检查，以覆盖旧 pending、显式 producer 和异常交错。
3. 出队后发现单个冲突，保留既有 ID 的原请求，逐请求标识冲突；同批无冲突请求继续 merge 或原样重排。
4. 暂时容量不足可有界重排，保持原 ID、payload 和签名事实；重排不再次消费 nonce、不变更业务字段、不重复预留字节。
5. 无法安全重排时，Node 保留按请求关联的显式失败及原因，由 runtime adapter 交付已有状态/错误表面；仅设置 `last_error` 不满足该义务。
6. commit、显式拒绝或去重处置在各自完成点释放一次预算；队列/engine/重排之间转移不得泄漏、重复释放或产生负计数。

内部处置结果是 Node 请求管线的事实，不是新的链上 receipt。已提交事实不得由后续队列错误撤回。
transfer 可用既有 `Failed/error_code/error` 表达确定拒绝；其他 producer 的兼容映射必须由 runtime owner 明确并验收，不能伪装为 transfer。
本专题不新增取消或补偿协议；未被既有 authority 支持的取消请求不改变已接纳动作。

<a id="des-http-h3-independent-query"></a>
### 5.3 P0 独立同步与响应

1. `tx/txs/blocks/search` 在生成响应前进入同一 runtime ingestion，不能要求客户端先调用 status、overview 或 history。
2. 沿既有顺序先取得 tracker，再短暂取得 runtime 并单次 drain；释放 runtime 后才取得 Explorer。Node/worker 不反向取得这两个 store。
3. 同一批次先完成 Explorer 与 tracker 两侧幂等合并；每个消费者确认处理完成后，统一入口才释放该批次的同步责任。
4. 再刷新允许按时间变化的生命周期，最后读取相应索引、过滤和序列化响应；不要先制造 timeout 再依赖另一个请求纠正。
5. 同步失败返回结构化失败，不把旧索引包装为已完成同步；重复查询可重试同一批次，不产生重复记录。

一次读取的提交观察边界是单次 drain 在 committed-batch 队列锁内取得批次快照的时点：该时点前已提交且仍在声明保留范围内的批次必须完成本次同步；之后并发发生的提交可以在后续读取体现。

worker 也通过该内部队列锁发布批次，外层 runtime 锁本身不冻结提交；各次响应不构成跨请求原子快照，见[批次读取][src-drain]与[批次发布/保留][src-retention]。

新同步接点复用 `tracker → runtime（释放）→ Explorer` 的方向；只持有 Explorer 的 helper 不得反向获取 tracker/runtime。
submit 在释放 runtime 后分别登记两个 store；这些登记依靠幂等合并抵抗交错，不通过持有 runtime 再等待 tracker 来“消除竞态”。
一次 drain 后尚未向两侧成功交付的数据由统一入口有界保留并优先重试；不得在未完成时继续 drain 造成无界积压。
批次暂存沿用 Node committed-batch 的容量上限，但不得继承可淘汰历史热窗口的裁剪行为：已 drain 而未完成双消费的批次仍是待交付责任，不能按年龄或容量淘汰。取得批次前必须有足够暂存空间；空间不足时不继续 drain，保留已有待交付批次并显式暴露背压或错误。
持久化失败不撤回内存里的确认；dirty 索引可重试保存，不能谎称已具备 reload 保证。进程崩溃后的完整批次重放仍属既有恢复 authority。

## 6. 接口与数据合同

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| HTTP submit | handler → Node | 原签名 envelope、Node action ID；签名 nonce 独立 | admission 绑定检查与预算接纳是单次请求边界 | 200 仅 accepted；接纳前拒绝保留原错误语义 | 不更改现有签名域、payload 或账户授权 |
| 批次处置 | Node → runtime adapter | 原 action ID + payload/submitter 绑定 | 相同请求重投不重入队；处置可关联 | committed 或明确拒绝；暂态可重排 | 内部结构可实现，新增外部字段/路由须明确兼容 |
| committed ingestion | Node → tracker/Explorer | 原 `NodeCommittedActionBatch` 与 action/块关联 | 单 drain、双消费者、重复批次幂等 | 两侧完成才解除责任；失败保留重试事实 | 不改变共识 envelope 或链上 schema |
| accepted merge | handler → tracker/Explorer | 同一请求 action ID 与已有 metadata | 仅首次建立顺序项；确认优先 | 同一请求补缺；矛盾绑定显式失败 | 不抹除可读旧确认或替换区块关联 |
| P0 query | native/Web/direct HTTP → runtime | 原 `account_id/status/action_id/cursor/limit` | 同步后统一谓词、排序、计数、分页 | 现有结构化 error 与空结果可区分 | 保持路径、字段与单项匹配语义 |

查询响应的 `observed_at_unix_ms` 不升级为提交、最终性或同步 SLA 证明。
批次解码按既有版本边界处理；已知 runtime payload 不得以解码失败或跳过逻辑伪装为已完整消费。

## 7. 状态、事务与持久化

<a id="des-http-h2-lifecycle"></a>
### 7.1 幂等状态合并

以下状态规则适用于两个 store 各自已有的字段。两侧共同保持 action 确认状态和顺序索引；block height/hash 与 tx→block 关联由 Explorer 维护，不要求为 `TrackedTransfer` 或既有 transfer 状态 DTO 新增区块字段。

| 当前记录 | 到达事件 | 目标处理 |
| --- | --- | --- |
| 不存在 | 合法 accepted | 新建 Accepted 与一次顺序索引 |
| Accepted/Pending/Timeout | 相同请求 accepted 重复或迟到 | 补缺失元数据，不重置时钟、不回退状态、不重复入索引 |
| Confirmed | 相同请求 accepted | 保留 Confirmed 与已有确认相关字段；Explorer 保留 block height/hash，仅补缺失提交元数据 |
| 不存在/Accepted/Pending/Timeout | 匹配的 committed batch | upsert Confirmed，Explorer 补块关联，各自顺序索引仍唯一 |
| Accepted/Pending | 超过原时间阈值且未观察到确认 | 按现有规则 Pending/Timeout；不是资产执行失败证明 |
| Confirmed/确定 Failed | 时间刷新 | 不退回 Accepted/Pending/Timeout |
| 任意已有记录 | 相同 ID 的矛盾 payload/submitter/块关联 | 保留原事实并显式诊断；不能拼接成另一笔请求 |

Timeout 仅为等待确认的观察超时；晚到匹配 committed 可将其纠正为 Confirmed。
Failed 只能来自已明确的拒绝/失败处置，不能由经过时间推断；同请求同时出现确定拒绝与有效 commit 是矛盾事实，须报告并按权威证据核对。
tracker 的 `by_action_id/action_order` 与 Explorer 的 action→tx/tx→block 索引分别维持唯一性；不使用数组末尾插入次数充当请求数。
accepted 与 committed 的可变字段需区分“未提供”与“已经确认”；空值不能覆盖确认材料，冲突值不能静默替换。

### 7.2 提交点与恢复边界

Node admission 是请求接纳点，Node committed batch 是此观察管线的确认输入，两者都不由 HTTP 连接存活与否决定。
两个 store 是可重建的派生视图；单次 ingestion 负责两侧的幂等更新，不声称跨组件数据库事务或 exactly-once 全局执行。
Explorer 沿用现有 `explorer-index.json` 的版本和有限保留策略；修复应持久化合并后的完整记录，并验证 reload 后状态与块关联一致。
历史上已经被错误覆盖的记录不能靠本次 merge 规则自动修复；只在仍有可验证 batch/索引材料时对账，不补造缺失确认。
重启 ID 边界与持久索引接线未核定前，不得把内存 allocator 修复宣称为完整 H1；详见 §10、§12。

## 8. 部署、安全与运行约束

- 适用普通 chain HTTP runtime；guarded WorldService 分支和正式网络激活仍遵守各自 authority，本设计不启用其拒绝的 legacy submit。
- 调用方仍须经过原签名、账户/玩家绑定、余额、nonce 和 admission 检查；ID 分配与重排不得扩大权限。
- 使用隔离 runtime、测试账户和本地临时索引验证，不使用真实资产或外部付费 provider。
- 不调整既有 payload、队列、committed 热窗口、Explorer 保留量和 timeout 默认值；更改这些边界须单独给出容量依据。
- 锁、解码、预算、身份或持久化错误必须可定位；诊断不得包含私钥、凭据或未经必要裁剪的敏感 payload。
- 本地测试和原子文件替换不能证明跨故障域持久化、公开可用性或任意文件系统的 crash durability。

## 9. 质量与容量

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 同 tick 多入口 | 单 Node、合法 transfer/main-token/gameplay，同 worker tick 前提交 | ID 不同、每笔结果可关联 | H1；数量/字节沿用 Node 配置 | §11 H1 handler 组合 | planned，未运行 |
| 冲突与重排 | 冲突请求加第三笔正常请求；覆盖队列/engine 饱和 | 不连带丢弃，预算守恒，后续 tick 恢复 | 现有容量与字节上限，不新增性能阈值 | Node 定向故障注入 | planned，未运行 |
| 迟到登记 | 两种固定 barrier 交错；超过原 timeout 后 reload | 两侧 Confirmed，索引唯一，Explorer 块关联保留 | H2 状态不变量 | tracker/store + handler 测试 | planned，未运行 |
| 直接 P0 查询 | 无其他同步消费者；内部 commit 后仅 P0 访问 | 同批可见，重复查询无重复插入 | H3；不定义毫秒 SLA | P0-only 定向测试 | planned，未运行 |
| 组合过滤 | action/account/status 匹配与不匹配组合，分页边界 | 同一匹配集驱动 items/total/cursor | H4 和既有排序、limit/cursor | store/runtime/native/Web 请求测试 | planned，未运行 |

<a id="des-http-h4-filters"></a>
### 9.1 过滤求值与分页

HTTP 参数为 `action_id`、`account_id` 和 `status`；账户条件不新增 `account` 别名。每个已提供条件形成一个谓词，结果满足 `action_match AND account_match AND status_match`；缺省条件相当于不限制该维度。
action ID 索引只缩小候选集，不提前返回最终结果；账户保持原发送方/接收方匹配含义，状态保持原解析规则。
所有谓词完成后按既有排序得到结果序列，`total` 取未分页匹配数，再应用 cursor/limit 并计算 next_cursor。
回显必须使用实际执行的规范化条件；action 命中而账户或状态不匹配时 `items=[]/total=0`。
native/Web 请求不得在存在 action ID 时清除其他条件；非法条件沿用原结构化拒绝，不退回无过滤查询。

## 10. 兼容、迁移与回滚

本次只写文档；HTTP/schema/索引版本、配置和部署保持基线行为，不由文档合入自动激活目标设计。
实施可分项进行，但应先盘点全部 Node producer 与旧索引身份，再接入共享 allocator/处置，最后完成幂等 merge、统一同步和过滤。
H2/H3/H4 可独立验证；H1 身份与可靠处置分别验收，不能因为某项修复而替其他项标完成。
旧消费者仍能读取原字段；新增非 transfer 结果通道或持久 allocator 数据格式时，runtime owner 必须先在本设计补明确切换和旧数据策略。
重启不得静默删除旧 Explorer 索引来制造 ID 唯一，也不得用窗口内最大 ID 冒充完整历史高水位。
回滚目标需在实现 PR 指定实际已知基线；回退二进制不能撤回 committed 事实或修复历史误投影。
若新实现形成旧程序无法安全解释的状态，停止该写入入口并由 runtime owner 明确处置；不以清状态作为默认回滚。

## 11. 验证设计与可追溯性

验证分为文档结构检查、现有回归和新增行为场景；文档检查不能证明 H1–H4 实现完成。
既有测试文件以下均相对仓库根；实现 PR 记录实际 source/tested commit、配置、隔离环境、命令、退出码及产物。

<a id="http-validation"></a>
### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: http-h1-identity](chain-http-submit-explorer-contract.prd.md#http-h1-identity) | [des-http-h1-identity](#des-http-h1-identity) | 同 Node 全部 HTTP producer 唯一，连续入口和原鉴权不回退 | [测试分层与执行手册](../../../testing-manual.md)的 S4/S9A 隔离合同层；现有 `crates/oasis7_chain_runtime/src/host/main_token_submit_api_tests.rs::claim_main_token_submit_handler_accepts_signed_request` 与 `transfer_submit_api_tests.rs::transfer_submit_handler_rejects_missing_signature` 等签名回归；新增同 tick 多真实 handler、其他 producer 冲突、溢出及旧索引重启场景，planned；同候选隔离 runtime | #4450 H1 + 实现 PR 的命令/结果/产物 | 本文未运行；重启接线未决，fixture 不证明全网唯一 |
| [professional_acceptance: http-h1-disposition](chain-http-submit-explorer-contract.prd.md#http-h1-disposition) | [des-http-h1-disposition](#des-http-h1-disposition) | 冲突不吞健康请求，重排/拒绝和预算一致 | [S4 Node 验证手册](../../../testing-manual.md)；`crates/oasis7_node/src/tests_action_payload.rs::{runtime_dequeues_actions_with_engine_capacity_limit,pos_engine_apply_rejected_decision_surfaces_requeue_overflow_instead_of_dropping,submit_consensus_action_payload_rejects_queue_saturation}` 既有入口；新增冲突加第三请求、字节余额、后续 tick 与逐请求结果 oracle，planned | #4450 H1 独立处置证据；Node/runtime 测试输出 | 现有单测名称不证明 HTTP 组合；非 transfer 反馈仍须接线 |
| [professional_acceptance: http-h2-lifecycle](chain-http-submit-explorer-contract.prd.md#http-h2-lifecycle) | [des-http-h2-lifecycle](#des-http-h2-lifecycle) | 两侧迟到 accepted 保留确认、索引唯一、reload 一致 | [S9A 投影验证手册](../../../testing-manual.md)；现有 `crates/oasis7_chain_runtime/src/host/transfer_submit_api_status_history_tests.rs::transfer_status_and_history_endpoint_report_confirmed_record`、`explorer_p0_store.rs::persists_compact_json_and_reloads_block_transfer_and_action_index`；新增入队后/两store之间 barrier、超时及重复登记，planned | #4450 H2；两 store 状态/顺序、Explorer 区块断言和 reload 输出 | 未运行；不证明历史已污染数据自动修复或永久保留 |
| [professional_acceptance: http-h3-independent-query](chain-http-submit-explorer-contract.prd.md#http-h3-independent-query) | [des-http-h3-independent-query](#des-http-h3-independent-query) | P0-only 同批可见、单 drain 双消费、失败可重试且不死锁 | [S9A API 投影验证手册](../../../testing-manual.md)；保留 `crates/oasis7_chain_runtime/src/host/transfer_submit_api_explorer_tests.rs::explorer_p0_blocks_txs_tx_search_queries_return_expected_payloads` 的 status-first 路径（先轮询 transfer/status 至 Confirmed，且 NoopExecutionHook 不证明资产/世界执行）；新增禁止 status/overview/history 的 P0-only、重复查询、两侧交付/锁/持久化失败场景，planned；Node 热窗口回归见 `tests_action_payload.rs::runtime_committed_batches_respect_hot_window_limit` | #4450 H3；handler 响应与 drain/索引断言、失败恢复结果 | 未运行；无数值 SLA、跨重启重放或全历史证明 |
| [professional_acceptance: http-h4-filters](chain-http-submit-explorer-contract.prd.md#http-h4-filters) | [des-http-h4-filters](#des-http-h4-filters) | AND、回显、排序、total 与分页一致，真实客户端保留条件 | [S4/S6 分层验证手册](../../../testing-manual.md)；现有 `crates/oasis7_chain_runtime/src/host/explorer_p0_store.rs::query_txs_pages_sorted_refs_and_clones_only_page` 与 `transfer_submit_api_explorer_tests.rs::explorer_transactions_reject_invalid_status_filter`；新增全匹配/账户错/状态错/单action/cursor矩阵，并核 native/Web 实际请求，planned | #4450 H4；store、runtime、客户端参数与结果证据 | 未运行；UI 默认操作顺序不能替代 API 矩阵 |

新增测试命名由实现 owner 确定；上表的新增场景是计划，不伪造仓库已有函数或通过记录。
预算回归另保留 `submit_consensus_action_payload_rejects_zero_action_id`、`submit_consensus_action_payload_rejects_payload_over_limit`；正常 accepted/pending/confirmed、真未提交 timeout 和鉴权拒绝均不得退化。

## 12. 决策、长期风险与未决问题

| 决策/问题 | 选择与理由 | 正负后果及边界 | owner / 复核触发 |
| --- | --- | --- | --- |
| 身份分配责任 | Node 共享分配与绑定检查；不采用入口错开常量 | 覆盖新增 producer；需盘点全部显式 ID 调用方 | runtime owner；新增 producer、保留身份或 Node 初始化变化时复核 |
| 重启与旧索引 | 必须防止 ID 与仍存活记录混同；持久高水位或既有隔离接线尚未冻结 | 不新增未经设计的 epoch/schema，也不承诺全历史；该边界未解不能完成 H1 | runtime owner；H1 实现接入启动/restore 前，提交格式、旧数据、失败/回滚设计及测试 |
| 非 transfer 处置反馈 | Node 保留关联处置，由实际入口 adapter 负责消费者可读结果 | 不把 transfer tracker 冒充通用 action store；无反馈入口时义务仍未完成 | runtime + viewer owner；H1 producer 盘点发现缺通道时，先核对既有 DTO/查询并明确兼容方案 |
| 同步模型 | 复用统一请求前 ingestion、单 drain 双消费者及既有锁方向 | 不引入新后台调度；需处理 drain 后失败和有界暂存 | runtime owner；同步入口、锁拓扑、保留策略或消费者变化时复核 |
| 状态优先级 | committed 观察优先于迟到 accepted；Timeout 可被迟到确认纠正 | 避免误把超时当不可逆失败；矛盾事实仍需明确诊断 | runtime/QA owner；状态枚举、确认 authority 或持久版本变化时复核 |
| 过滤实现 | action 索引只缩候选集，复用统一 AND 谓词 | 保持原参数而改变错误结果；客户端必须保留全部条件 | runtime/viewer owner；新增条件、排序或分页协议时复核 |

本次选择不改变共识算法、签名域或权限，采用本节决策表即可；若后续需要新持久协议、跨进程身份或难回滚 schema，按系统设计规范另补相应 ADR。
文档通过检查或合入不关闭四项行为义务；实现和验证仍分别回链 #4450，不能以未来任务占位宣称完成。

[src-transfer-id]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/transfer_submit_api.rs#L53
[src-main-id]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/main_token_submit_api.rs#L21
[src-gameplay-id]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/gameplay_submit_api.rs#L23
[src-node-submit]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7_node/src/node_runtime_core.rs#L321-L397
[src-merge]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7_node/src/consensus_support.rs#L36-L88
[src-dequeue]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7_node/src/consensus_support.rs#L242-L253
[src-tracker]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/transfer_submit_api.rs#L150-L181
[src-explorer-accepted]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/explorer_p0_store.rs#L148-L216
[src-p0-route]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/transfer_submit_api.rs#L476-L483
[src-sync]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/transfer_submit_api.rs#L905-L1003
[src-query]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7/src/bin/oasis7_chain_runtime/explorer_p0_store.rs#L465-L528
[src-drain]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7_node/src/node_runtime_core.rs#L436-L441
[src-retention]: https://github.com/eng-cc/oasis7/blob/e01e29064517f4bf375767a19a09802335828249/crates/oasis7_node/src/node_runtime_batch_retention.rs#L13-L56
