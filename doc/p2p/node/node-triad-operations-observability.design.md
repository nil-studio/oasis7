# 三节点运维观测合同设计

- 对应需求文档: `doc/p2p/node/node-triad-operations-observability.prd.md`
- 执行与验证事实: Git、PR、实际 CI 与评审记录；Issue/Project 按需使用。

## 设计定位

以 repo-owned 脚本保持“一个控制机采样三节点、在同一 run dir 合并证据”的模式。summary 只消费 runtime 发布的 status truth；它将 host、chain、traffic、WASM 与 bounded reachability 拼接为诊断面，不能成为第二套 consensus 或 reachability 真值。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | 当前采集失败与历史成功分开，窗口以显式 as-of 解释；不由旧数据宣称当前可用 | [新鲜度归约](#report-freshness) | runtime status producer 与 ops collector | 不证明运行时服务闸门或现场恢复 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[角色观测](node-triad-operations-observability.prd.md#ops-profile-readiness) | 采集前 pending、严格递增序号、同尝试条件终态与验证读取 | [外层持久尝试](#durable-observability-attempt) | ops collector／report publisher | 不证明独立 collector、角色权威就绪、真实 SSH 或物理回滚 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[角色观测](node-triad-operations-observability.prd.md#ops-profile-readiness) | host 采样前 pending、独占 run、同类型状态与条件终态 | [host 持久尝试](#host-durable-attempt) | host collector／report publisher | 不证明 snapshot／traffic 持久尝试、角色就绪、真实 SSH 或物理回滚 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[角色观测](node-triad-operations-observability.prd.md#ops-profile-readiness) | 区分实例存活、读取范围、授权、成员采用和签名/追加阻断 | [角色就绪投影](#report-profile-readiness) | 各 profile producer、ops 与 QA | 目标字段/适配仍待实现；不扩大 triad 拓扑支持或新增自动操作 |
| [独占 run](node-triad-operations-observability.prd.md#observability-run-contract) | 写入前原子独占；冲突写前拒绝/重分配，同 run 工件所属一致。 | [编排目标](#run-exclusive) | runtime / blockchain ops / QA；上游各自 owning module | 未运行目标故障/并发回归；不证明 producer 修复、真实 SSH 或现场恢复。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [参数路由](node-triad-operations-observability.prd.md#observability-run-contract) | service/status/shared target 先校验再按消费者分配，非法参数写前失败。 | [编排目标](#run-routing) | ops collector／QA；子工具各自 owning module | 限受支持 triad 参数；不证明任意拓扑、host 内部失败续采或现场恢复。产品适用性：N/A；理由：本行为 operator 技术接口，不新增玩家承诺；范围：受支持的三节点参数路由，不替代现行可用性义务。 |
| [失败后报告](node-triad-operations-observability.prd.md#observability-run-contract) | 首个／后续／全部节点采样失败保留不可用样本、继续其余采样并生成非零终态。 | [编排目标](#run-failure-report) | ops collector／report publisher／QA | 不证明独立入口的持久 pending、目录并发安全或真实故障域恢复。产品适用性：N/A；理由：本行为 operator 诊断接口；范围：本地采集与报告，不替代现行产品可用性义务。 |
| [持久错误](node-triad-operations-observability.prd.md#observability-run-contract) | reward collector/probe state 写入失败独立保留 last_error/degraded，不因旧 report、无新 epoch 或 invariant 正常清错。 | [编排目标](#run-worker-error) | runtime / blockchain ops / QA；上游各自 owning module | 未运行目标故障/并发回归；不证明 producer 修复、真实 SSH 或现场恢复。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [选用范围](node-triad-operations-observability.prd.md#ops-selected-scope) | 本轮明确候选、inventory、用途；旧 fleet 退出不把旧未验证结果升级为通过。 | [报告目标](#report-scope) | runtime / blockchain ops / QA；上游各自 owning module | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [OPS-AC-1](node-triad-operations-observability.prd.md#ops-report-contract) | 真实 traffic envelope 保值，未知 schema/不可用/缺文件显式诊断，仅支持格式可 adapter。 | [报告目标](#report-envelope) | runtime / blockchain ops / QA；上游各自 owning module | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [OPS-AC-2](node-triad-operations-observability.prd.md#ops-report-contract) | 最新 attempt、最近成功、窗口截止独立；显式 as-of 保留合法历史且不伪造当前覆盖。 | [报告目标](#report-freshness) | runtime / blockchain ops / QA；上游各自 owning module | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [OPS-AC-3](node-triad-operations-observability.prd.md#ops-report-contract) | 真实窗口 nearest-rank p99 与 max、样本量分别表达。 | [报告目标](#report-percentile) | runtime / blockchain ops / QA；上游各自 owning module | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |
| [OPS-AC-4](node-triad-operations-observability.prd.md#ops-report-contract) | storage 目录并集、GC 状态与不足信息按上游解释，报告不授予发布或 GC 正确性。 | [报告目标](#report-storage) | runtime / blockchain ops / QA；上游各自 owning module | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。  产品适用性：N/A；理由：本行是旧接口维护或 operator 技术观测合同，本身不新增玩家承诺；范围：本行所列专业机制，不免除现行产品恢复、经济或可用性义务。 |

## 问题、目标与当前差距

- Owner role：`blockchain_ops_engineer`；runtime 维护 status/worker 真值，QA 判读适用证据。
- 审读基线：`edf61e801e4888087c2e65a402011088bd1c50b2`（2026-10-15）。目标是同一候选、inventory 和窗口下可诊断的 operator 报告；下列编排条款是应满足的目标，不是已实现声明。
- 当前外层 [monitor](../../../scripts/p2p-real-env-observability-monitor.sh) 按[持久尝试](#durable-observability-attempt)分配独占 run 并条件发布终态；独立 host 入口按[host 持久尝试](#host-durable-attempt)取得自己的发布保护；snapshot／traffic 入口仍未取得该保护。参数按[三节点消费者路由](#run-routing)分配；不接受通用透传，也不将 snapshot 的显式多节点清单混入固定 triad 的 host／traffic 证据。
- 外层 monitor 捕获子阶段退出，继续可独立运行的采集并发布失败诊断。[host monitor](../../../scripts/p2p-real-env-host-monitor.sh) 按[失败样本合同](#run-failure-report)逐节点记录成功或失败，继续剩余节点／样本，并在报告发布后以非零退出保留本 run 的采集失败。
- [monitor 测试](../../../scripts/p2p-real-env-observability-monitor.test.sh) 已包含外层实际入口的 stub 并发／故障及真实 merger 集成；[host 测试](../../../scripts/p2p-real-env-host-monitor.test.sh) 包含实际采集器的离线 I/O 故障回归。这些测试不证明真实 fleet 或物理故障域恢复。
- 表中标记“产品适用性：N/A”的旧工具行，仅描述目录隔离、参数路由等专业 operator 机制，本身不新增玩家功能或经济承诺；此说明不适用于明确承接 REQ-DCS-005 的新鲜度与角色观测行。storage/reward 的 producer 义务仍由各自 authority 拥有，本文不改变计算规则或恢复承诺。
- 当前 summary 的 `summarize_storage` 直接累加 `bytes_by_dir`，且将非 None/success 的 GC 状态均判作失败；与 OPS-AC-4 的目录并集、GC 多状态解释存在差距，仍需独立实现与正负例核验。
- 剩余风险：独立入口的目录碰撞、输出不可写和未支持的 producer 输入仍可使本轮报告不可用；未知 schema、过期窗口与 producer 错误不能用健康数值冲淡。源码审读不证明故障已修复，也不验证现场 fleet。

流量归约按[当前新鲜度策略](#traffic-freshness-v2)保留最新失败和历史成功，以显式边界及年龄／缺口预算归约，并在合并时重新核验年龄。外层编排的持久尝试与失败发布另见下节；snapshot／traffic 等其他独立 collector 的持久尝试和 profile producer 差距仍存在；本节不刷新历史现场证据，不能从流量新鲜度字段推断授权、成员或签名状态已经接线。

<a id="数据流"></a>
## 边界与数据流

`health/status + host/process + traffic window + WASM window -> snapshot/host/summary helpers -> JSON + Markdown artifacts -> canonical attempt/generation validation -> operator/QA evidence review`

每层都保留输入缺失、reset、degraded 和 blocked 状态。restart/replay/reorg 的 lifecycle persistence 由 runtime status/evidence contract fail closed；monitor 不能通过重启或补写历史数据掩盖它。

<a id="observability-run-design"></a>
## 编排责任与终态输出

以下为完整目标状态；外层已实现子范围见[持久尝试](#durable-observability-attempt)，不能据此宣称独立子采集器或现场环境均已满足。

<a id="run-routing"></a>
1. monitor 解析并验证配置；按 snapshot/host/traffic/WASM 的受支持输入分别构造参数，不把同一参数数组无差别转交所有子工具。

   外层入口只承接兼容三节点标签 `local_node`、`sequencer_ecs`、`storage_ecs`。解析采用显式选项集合；未知选项、空值、缺值和非法数值在预留尝试、创建输出与启动 collector 前以退出码 2 拒绝。数量、秒数、窗口和流量 top／retention 使用 ASCII 十进制正整数，上限 2147483647；前导零规范化为十进制字符串后再传递，避免 Bash 八进制及溢出。as-of 允许 0 至 9223372036854775807，仍须满足不晚于当前 UTC 的约束。重复受支持选项保持命令行次序，由对应消费者沿用最后一次值，每次出现均验证取值。字符串值必须非空且不以 `-` 开头；相对路径可用 `./` 前缀消除歧义。路径、service、URL 按独立 argv 元素传递，不在外层求值。`--help` 只说明用法，不预留尝试或采样。

| 参数组 | snapshot | host | traffic |
| --- | --- | --- | --- |
| `--sequencer-target`、`--storage-target` | 是 | 是 | 是 |
| 三节点的 `--*-service` | 是 | 是 | 否 |
| 三节点的 `--*-status-url` | 是 | 否 | 是 |
| 三节点的 `--*-storage-path` | 否 | 是 | 否 |
| 三节点的 `--*-health-url`、`--*-env-file`，远端的 `--*-public-status-url`／`--*-public-health-url`，`--world-id` | 是 | 否 | 否 |
| `--top-n`、`--history-retention-minutes` | 否 | 否 | 是 |

外层的 sample／interval 选项继续分别映射 snapshot／host 与 traffic，SSH timeout 传给三者；流量年龄／缺口及历史回放按[新鲜度策略](#traffic-freshness-v2)独立分配。WASM 只消费本 run 的 snapshot 样本，不接收节点采集参数。snapshot 的 `--node` 可改变节点集合，但 host／traffic 和合并报告尚不支持该拓扑；外层必须明确拒绝它，不能仅传给 snapshot 而让其他模块静默采默认节点。子工具的 `--summary-only`、`--history-path` 和未实现的 `--observer-*` 别名不属于外层接口，不能通过通用透传绕过；外层历史流量入口仍为配对的 `--traffic-history-path` 与 `--as-of-unix-ms`。本路由不改变各 collector 的 SSH／HTTP 行为、凭据来源或 standalone 接口。

<a id="run-exclusive"></a>
2. 写入前原子分配独占 run 目录，snapshot 单独调用也承担同样责任；冲突在写前拒绝或重新分配，不覆盖已存在目录。外层与子采集器记录同一所属关系。
<a id="run-failure-report"></a>
3. 捕获采集失败并保留结构化不可用样本；host SSH 失败不跳过后续节点和最终报告。终态汇总同时保留非零退出、失败节点、仍有效的其他采样和 underlying snapshot 的 blocked。

Host 样本 schema 2 对每次已执行的节点采样保存 `sample_fetch_ok`、`sample_index`、实际完成时点与固定 `stage`／`fetch_error` 类别。local probe、SSH、空响应、缺字段或数值损坏均形成失败行；不执行采样返回的 shell 文本，不把原始 stderr、环境或凭据写入公开诊断。每次采样单独解析，不能继承上一节点／上一轮的变量。一次失败不跳过剩余节点或请求的后续轮次；写入／归约本身失败仍非零，不承诺磁盘不可写时能生成终态工件。

Host summary 分开输出每节点 `latest_attempt` 与历史 `last_success`，保留总尝试数、成功／失败数。最新失败时 `available=false` 且当前 `latest` 为空，历史成功与峰值不能替它填当前读数；失败后新的有效采样可恢复该节点的当前可采集性。`aggregate.complete` 仅在本 run 所有请求采样均有效时为 true，`expected_sample_count` 记录请求总数，`attempted_count`、各节点计数及 `failed_sample_count`／`failed_nodes` 交叉核对并保留窗口失败；即使后续恢复，本 run 仍非零退出并保持 incomplete。缺新标志的已支持 legacy 成功历史继续按旧格式解释，不因此取得新鲜度或角色就绪保证。

合并报告保留 host 聚合诊断与每节点尝试／历史引用；schema 2 缺失或未满足 complete 时整体 blocked，未知 host schema 同样不可判定。underlying snapshot 的 blocked 不被资源正常值或后续采集恢复覆盖。外层持久尝试即使因 host 非零而失败，仍可在核对 run ID／目录后保留本 run 已成功生成的 merged 诊断，附加有界阶段错误与 unavailable 标志；错误 run、缺失或损坏报告不能被采纳。host 独立调用的目录隔离及持久 pending／条件发布见[host 持久尝试](#host-durable-attempt)；真实 SSH 故障域验收仍不在此子范围。

4. 汇总仅引用本 run 的实际文件；失败细节限制为 label、时间、阶段和受限错误类别，不输出凭据或完整环境。
<a id="run-worker-error"></a>
5. reward state 写入错误独立于数值发布与 invariant：采集窗口按实际 last_error/degraded 解释，不用已有 report 或无新 epoch 清错。producer 只有在对应持久化真正恢复后才清除该错误；本设计不更改持久化格式或奖励计算。

<a id="ops-report-design"></a>
## 选定范围、输入适配与窗口归约

承接 [当次范围](node-triad-operations-observability.prd.md#ops-selected-scope) 与
[OPS-AC-1–4](node-triad-operations-observability.prd.md#ops-report-contract)。沿用当前受支持工具，
不把本文解释成 triad 脚本已支持任意拓扑或新增部署控制面。

<a id="report-scope"></a>
1. 在现有 run 配置/报告中明确选定 candidate、inventory 和用途；历史样本只按原身份、窗口引用，不从旧包重新排部署。
<a id="report-envelope"></a>
2. adapter 区分真实 envelope、status-only 和明确支持的旧平铺格式，归一化值、可用性、窗口及错误；未知输入保留原因，不静默变为零流量。
<a id="report-freshness"></a>
3. reducer 分别保留最新 attempt 和最近成功。各节点用同一显式 as-of 判窗口交集、age、stale 与 reset；当前失败不能由旧成功掩盖，合法历史窗口仍可离线重算。

   新输出保留 `as_of`、`latest_attempt_at/result/error`、`last_success_at`、`sample_age` 与窗口实际覆盖范围；字段命名在实现 schema 中版本化，不把旧 `available` 的含糊语义升级成当前健康。窗口终点不能自动退回最后成功时间，完整覆盖还须满足适用采样频率与最大允许缺口，两个稀疏端点不足以证明连续覆盖。配置明确 freshness/缺口预算，时钟回退或样本晚于允许 as-of 偏差时显示 unknown；同实例内时长优先采用单调时钟，跨主机仍保留墙钟不确定性。失败样本参与可用性与覆盖判定，不填充成功流量；历史 as-of 只选截至该边界已存在的样本。

   collector 的尝试/成功时点、producer 的实际 observation 时点、最后已验证状态或提交的时点分别保留。`sample_age` 以实际 observation 与 as-of 的差及其时钟不确定性解释；HTTP 200、重新读缓存或报告生成时间不能刷新旧 observation 的年龄。缺少可信 observation 时间时，该新鲜度为 unknown；已验证状态的年龄另列，不以“刚采集成功”替代状态已经追到当前可信边界的证据。
<a id="report-percentile"></a>
4. p99 从实际滚动样本计算 nearest-rank（升序 rank=ceil(0.99×N)），保留 p95/max 和窗口样本数。完整窗口要求按 producer 合同满足；样本不足时标明不完整/不可用，不将 max 作为 p99 替代值。
<a id="report-storage"></a>
5. report 将同一归约结果输出 JSON 与 Markdown；storage 的目录去重、GC availability 由存储合同提供，不在 report 再维护一套统计真值。

| 验收 | 目标回归与分层 |
| --- | --- |
| 范围选择 | 单个选定候选与实际 inventory 一致；旧 fleet 退出不产生新部署动作；工具验证不冒充现场健康 |
| OPS-AC-1 | 非零真实 envelope 经 adapter→merged report 保值；available=false、未知 schema、缺文件及受支持旧格式分别覆盖 |
| OPS-AC-2 | 00:00/00:10 成功，00:11–00:30 持续失败，as-of=00:30 的最近十分钟不得 full coverage；另测失败后恢复、全失败、正常满窗、reset 与显式历史 as-of |
| OPS-AC-3 | producer→adapter→status→report 验 128 样本反例；100/128 边界、空窗、滚动淘汰最大值分别检查 p99/max 与可用性，不能只对 summary 手工填字段 |
| OPS-AC-4 | 按存储正负例核对数值、告警与不足信息诊断；不增加 release gate 或自动 GC/恢复动作 |

回归可使用离线 fixture/stub，但需走实际适配和汇总链；每项记录实际执行结果，不能以设计矩阵证明实现已修。
真实采样与服务恢复声明仍按对应环境和当次授权独立验收。

<a id="traffic-freshness-v2"></a>
## 流量报告当前新鲜度策略

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；本节落实 [OPS-AC-2](node-triad-operations-observability.prd.md#ops-report-contract) 的流量采集／归约／合并子范围，不替代角色 producer、持久 pending 或 runtime admission。

流量报告 schema 2 在一次归约中为全部选定节点固定一个 `as_of_unix_ms`。`--as-of-unix-ms` 可显式选择历史边界，省略时使用调用时的 UTC 墙钟；窗口终点保持该边界，不退回最后成功时间。`--max-sample-age-secs` 默认 60 秒，`--max-sample-gap-secs` 默认 120 秒，均必须是正数；后者容纳单节点默认 60 秒睡眠及正常采集／归约开销，显式更小预算仍严格执行。显式历史边界为非负整数，且不得晚于调用时的 UTC 墙钟；未来查询在写入或采集前拒绝。历史重算只消费截至边界的证据，不能为了此次回放修改历史文件或删除较晚样本。`--summary-only` 无论是否指定 as-of 都通过独立 read-only-history 标志禁止修剪历史；省略 as-of 仍按当前时间判断新鲜度，不因此绕过未来证据检查。

每节点分开输出 `latest_attempt`、`last_success` 和 `freshness`：采集成功／失败、错误类别、各自实际采集时点、采集年龄与最后成功年龄保留。HTTP 成功但空响应、无效 JSON 或不支持的 status 形状形成失败样本；三节点按各自采集完成时点记录，不共用串行请求前的旧时间。producer 的 `status_observed_at_unix_ms` 单独计算年龄；缺失、无效或未来观察为 unknown，旧观察仍为 stale，HTTP 200、报告生成或重新读文件不得刷新它。

当前可采集性与历史数值分开。最新失败、陈旧、时钟不确定性不能以旧成功显示当前 available；仍保留历史成功身份／时点及合法计数信息。计数差只使用同实例、同域与相容 counter epoch 的成功材料，失败不产生零流量样本，也不填补覆盖。完整窗口必须同时满足左右边界、成功样本最大间隙、无采集失败／reset 和当前新鲜度；两个远隔端点不能仅凭跨度成为完整覆盖。乱序或倒退的采集时间不得被排序悄悄修复成健康；保留历史中仍存在该异常时持续 unknown，普通请求失败后恢复成功与时钟异常恢复不是同一承诺。

合并报告使用一次固定的报告 as-of 重新核对采集和 producer 年龄，不能永久信任子报告曾经的 fresh 字符串。它保留子报告原窗口终点，明确区分报告边界与原采样窗口；较旧窗口超过缺口预算后不得继续显示当前完整。显式历史重算时，两层使用同一 `--as-of-unix-ms`；子报告晚于所选报告边界、身份不相容、缺少或未知 freshness schema 均保留 unknown／告警。JSON 与 Markdown 消费同一结果。外层 monitor 只把流量年龄／缺口参数传给流量采集器，把显式 as-of 同时传给流量与合并报告，不误传给 host／snapshot 工具。外层显式 as-of 必须配对 `--traffic-history-path`，以 summary-only 读取已有流量历史；缺少历史入口须在创建输出或采集前拒绝。该回放仅覆盖流量证据，host／snapshot 仍按当前采集解释，不宣称整个合并报告是历史现场快照。

兼容收紧：旧报告仍可作为历史诊断读取，但没有 schema 2 新鲜度证据时不能表示当前健康；历史 CLI 调用需显式 as-of 才能重放旧窗口。该策略仍依赖采集和 producer 自报时钟，不证明绝对可信时间、远端身份认证、现场全 fleet 健康或角色就绪。外层编排的持久尝试见[条件发布](#durable-observability-attempt)；独立 host 的持久尝试见下节，snapshot／traffic 的持久尝试、角色就绪与其他已记录编排缺口仍待实现，不因本切片通过而提升支持声明。

<a id="durable-observability-attempt"></a>
## 外层编排的持久尝试与条件发布

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；本节落实 [OPS-AC-5](node-triad-operations-observability.prd.md#ops-profile-readiness) 中外层 collector 编排的持久尝试子范围。入口为 `scripts/p2p-real-env-observability-monitor.sh`；独立 host 的持久尝试见[下节](#host-durable-attempt)；snapshot／traffic 的持久尝试及角色权威就绪仍是后续范围。

当前外层入口使用 POSIX OS 锁；锁能力不可用时在采集前拒绝。每个规范化输出目录使用稳定的 OS 文件锁载体；锁文件不删除或按 PID／超时抢占，初始化锚绑定载体设备、inode 及在首个初始化锚前已 fsync 的随机持久标识，不能只凭可能复用的 inode 判断同一载体；已初始化目录中载体丢失、替换或身份不符时拒绝，不重新创建第二锁域。随机标识只在持有首初始化锁时写入原锁 fd；缺失、短写或不匹配的已绑定标识不自动修复。缺随机标识的中间版本初始化锚不支持静默升级，按损坏／不兼容状态阻断。锁内及发布前核对载体身份。锁只覆盖序号分配、状态读取和条件发布，不跨 HTTP／SSH 等采集过程。初始化先持久发布不可自动重建的初始化锚，包含独立序号高水位；每次预留先持久推进高水位，再写 pending 状态。高水位、规范状态及保留 run 的更高序号证据不一致时拒绝续办，不重用序号；初始化锚与 `attempt_state.json` 任一个缺失、损坏或序号不合法时阻断，不回退成初次序号。首次初始化中途崩溃也保持 unknown，需保全原件并修复原状态；不以新输出目录冒充原尝试已恢复。本机制依赖正常本地文件系统的 fsync／原子 rename，不证明全套尝试存储（含锚及保留 run）同时恢复旧备份、恶意篡改或物理回滚下的防回退。

兼容边界：不含 attempt 协议的旧输出可作为历史诊断，但 canonical read 不把它认作已验证当前尝试。首次启用新协议可初始化序号 1；已有版本化尝试工件但锚／序号状态缺失时必须阻断，不按旧格式重新初始化。独立 collector 的旧接口不因此取得外层发布保护。

在任何 collector 启动前，锁内分配严格递增的序号及唯一 attempt ID，独占创建各自的 run 目录，并将 `pending`、开始时点、候选和配置／inventory 指纹、run 位置写入规范状态。指纹只保存摘要，不保存秘密参数；其中候选指编排脚本版本，不替代 producer 的实际运行实例／构建身份。随后持久发布带同一 generation 的 pending JSON／Markdown；任一步失败均不启动 collector，并非零退出。固定墙钟或并发启动不会共用已有 run 工件。不同配置的历史成功只作为带原指纹的历史信息，不授予当前范围的就绪结论。

子进程退出码按固定阶段名保留。独立的后续采集仍可执行；缺失材料不能通过旧 run 路径补齐。收集完毕后生成本 run 报告，发布核对其 run ID／位置与预留尝试一致；错误 run 或旧报告不能被重新包装为新成功。退出失败或不能归约时生成有界的失败诊断，保持非零退出。`success` 只表示本次采集／归约执行完成，不等于节点 profile healthy／ready；原报告的 blocked、unknown 和其他模块状态不得被改写为健康。

终态在同一锁内比较最新 pending 的序号与 attempt ID。旧 A 晚于新 B 完成时，A 可保留自己的 run 工件，但不能更新 B 的状态、latest JSON／Markdown 或 last-success。只有对应的最新成功尝试才更新独立的 `last_success` 历史引用；新 pending／失败不清除历史。未能发布终态、SIGKILL 或状态恢复失败均保持 pending／unknown，不能由旧成功补成当前成功。

文件发布使用同目录唯一临时文件、文件 fsync、原子替换及目录 fsync。多个普通文件不能被当作一个原子事务：终态工件先写、规范状态最后提交，工件携带 generation，规范状态记录其内容摘要。当前读取使用 `scripts/p2p-real-env-observability-attempt.py read --out-dir <root>` 的验证路径，锁内核对规范状态、generation 与 JSON／Markdown 摘要；不一致、不可读或损坏时输出 pending／unknown 诊断并非零退出。此读取不修改原件，也不回退到 last-success。`canonical_state.validation_scope` 仅为 attempt 排序与工件完整性；`read_at_unix_ms` 与原报告 `as_of_unix_ms` 分开，读取不刷新采样／producer 年龄，不把旧报告升级为当前 profile 健康。退出 0 仅说明最新尝试执行成功且工件验证通过；当前健康仍须依据原窗口和适用新鲜度证据判断。根目录中的原始 latest 文件是物化工件，独立缓存或未验证旧副本不能作为实时状态权威；尤其不能假设磁盘发布失败会自动使已有任意副本失效。

离线验证必须驱动真实外层入口和实际发布 helper：正常成功、模块 blocked 仍保值、子进程失败后诊断、固定时钟并发、A 慢／B 失败或 pending、B pending 后 SIGKILL、终态／预发布写入失败、初始化中止、损坏／丢失序号状态和正常后续尝试。检查 canonical read 的 JSON／Markdown 一致、无秘密输出、last-success 独立、采集未提前启动及 superseded 发布无副作用。不执行真实 fleet SSH 或部署，不能据此关闭角色／协议及故障域验收。

<a id="host-durable-attempt"></a>
## 独立 host collector 的持久尝试

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；本节承接 [OPS-AC-5](node-triad-operations-observability.prd.md#ops-profile-readiness) 的 host 入口尝试发布义务。`p2p-real-env-host-monitor.sh` 复用上节的稳定锁、独立序号高水位、唯一 run、generation／摘要核验与终态条件更新，不另建第二套排序协议。

CLI 在写入前验证取值；正整数使用有界 ASCII 十进制并规范前导零，未知／缺值／空值在预留与采样前拒绝。host 在任何 local probe 或 SSH 前调用同一 helper 的 `begin --collector host`，将 pending 持久写入并分配 `runs/<sequence>-<attempt-id>`。仅在预留与 pending 工件发布成功后采样；各节点原始结果、失败行与本 run 的 reducer 报告都留在该独占目录，固定时钟或并发启动不能共用／截断既有目录。

发布协议只支持 `observability`（默认）与 `host` 两种当前消费者。初始化锚、状态及尝试绑定 collector 类型，begin／finish／read 在使用不匹配类型时拒绝；旧协议未存类型的状态只按既有外层 observability 解释，不能被 host 静默接管。无协议的旧 host 时间戳目录保留为历史材料，首次启用可分配序号 1，但不把旧缓存认作新协议的 last-success。已有版本化尝试但锚／状态缺失、损坏或序号不一致时仍阻断，不重新初始化。host 指纹覆盖当前 host 入口、采样 helper、reducer 与发布 helper 的代码和调用配置摘要，不能只记录外层编排版本；摘要不包含密码或环境原文。

host reducer 的 schema 2、三节点及请求／完成／失败计数、run ID 和目录必须与预留匹配。采样或 reducer 失败保留失败终态与非零退出；错误 run、未知格式或缺失报告不能被采用。最新尝试未完成或失败时，canonical host JSON 保持不可用／incomplete，而不是展示旧成功；source 仍有效时保留其节点诊断和历史引用。只有最新、采集／归约成功的对应尝试能推进独立的顶层 last-success。节点内部 last-success 与该集合尝试历史分开；资源告警不等于采集失败，执行成功不授予角色就绪。较早 A 晚于 B 的 pending／失败完成时只保留 A 自身工件，不改 B 的状态或 latest；SIGKILL、预发布／终态落盘失败与安全序号恢复失败仍按 pending／unknown 解释。

规范读取为 `python3 scripts/p2p-real-env-observability-attempt.py read --collector host --out-dir <root>`（Markdown 加 `--format markdown`）。它校验同类型状态、generation、报告身份及 JSON／Markdown 摘要，不修改原件或刷新测量年龄。根目录 latest 仍仅为兼容物化工件。外层编排遇到版本化 host 子存储时，必须通过该读取取得本次 child 诊断，读取非零保留 host 阶段失败；不能把 `runs` 父目录当作旧时间戳 run，也不能回退读取不匹配的缓存。无协议的受支持旧 child 输出仍可在外层当前独占 run 内按 legacy 消费，但不因此宣称具备 host canonical 验证。

离线验证驱动实际 host 入口和 probe／SSH 替身，覆盖 pending 先于 probe、SIGKILL、同钟并发、慢 A／新 B 失败或 pending、写入失败与无提前采样、锁／锚／状态丢失及摘要损坏、失败后恢复、不同 collector 误用与旧外层状态兼容；再执行实际外层 host→merger→canonical 消费。共享机制已有故障回归继续执行。这里只证明 host 的本地持久排序和工件完整性，不证明全存储物理回滚防护、snapshot／traffic 持久尝试、真实 SSH 或角色就绪。

<a id="report-profile-readiness"></a>
## 按 profile 归一化就绪与阻断

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；承接 [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) 与 [OPS-AC-5](node-triad-operations-observability.prd.md#ops-profile-readiness) 的节点观测子范围。

producer 发布实际 profile、world/chain、构建与实例身份，adapter 只消费其声明支持的 schema。公共诊断区保存当次尝试、最近成功、样本年龄及实际读取的可信边界；角色区分别读取 projection/full-state loaded cut、grant 权限状态、authority epoch、成员决定/排程/实际采用、签名持久化、P/R 与保护服务可用性。设计来源分别为 [完整同步](../design.md#des-p2p-full-sync-read-only-v1)、[治理与读取权限](../blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-membership-application)、[持久签名](../design.md#des-p2p-bft-signing-recovery-v1) 和 [受控故障域](../design.md#des-p2p-controlled-fault-domain-acceptance-v1)；报告不自己重新计算权威资格。

| 归约情形 | 报告行为 |
| --- | --- |
| 当次失败、仅有旧成功 | 标记本轮不可采集或陈旧，保留最后成功及年龄，不清空历史，也不显示当前 ready |
| proof 冲突、权限失效、成员未采用、签名落盘失败 | 保留 producer 给出的隔离/阻断及影响范围；CPU/网络正常不能覆盖 |
| profile 不支持某能力 | 明确 unsupported；只有该角色确实不需要的字段可标不适用 |
| 字段缺失或身份/实例不匹配 | 对受影响能力显示 unknown/不可判定，不以默认 true 或零差距补齐 |
| 后续采集恢复 | 仅更新实际重新取得的事实；采集恢复不自动解除 producer 的权限或安全阻断 |

容量报告保留当前占用/配置上限、接近边界的告警与最近真实恢复耗时；没有测量时明确未知，不把测试超时预算当生产 SLO。默认只输出有界角色/错误类别标签，不导出完整 grant、密钥、私有状态或高基数 peer 标识。观察结果不能直接改变 runtime admission，自动 remediation 仍不在范围内。

取得独占 run 后、启动任何 collector 前，在对应 candidate/inventory/profile 的报告范围内持久记录 `attempt_id`、单调分配的尝试序号、开始时点与 `pending` 状态，并原子推进 `latest_attempt`；该预发布失败则不开始采集并非零退出。序号分配与指针更新须在同一互斥/条件更新边界内完成，不以结束先后判断尝试新旧；序号状态无法安全恢复时显示 unknown，不能回退到旧成功冒充本轮结果。

终态报告以本 run 的实际采样生成，即使采集失败也原子发布本轮诊断并保留非零退出。终态只可通过 `attempt_id` 与序号的条件更新完成仍指向本 run 的 `latest_attempt`；较早启动的慢任务可保存自己的结果，但不能覆盖较新尝试。独立 `last_success` 只在相同报告范围与身份下，按实际成功样本的新鲜度推进，作为有明确年龄的历史引用。终态写入失败时保留 pending/终端错误与此前报告，不能伪造本轮完成；SIGKILL 留下 pending 时，后续读取显示未完成/unknown，不因历史成功转绿。

验证沿实际 collector→adapter→reducer→JSON/Markdown 路径，覆盖 OPS-AC-5 各角色反例、连续失败后恢复、缺字段/旧 schema、不同实例串样、两个稀疏端点与时钟回退；并分别注入先启动的 A 慢于 B 完成、B 预发布后被 SIGKILL、B 终态写入失败及序号恢复失败，验证 A/历史成功均不能覆盖 B 的最新失败或未完成事实。使用 stub producer 只证明该适配范围；真实授权撤销、BFT 或 P/R 故障演练仍由各机制的验证合同负责。

<a id="安全与运行约束"></a>
## 安全、风险与运行约束

只采样资源和状态，不写 SSH password、private key、seed、完整 env 秘密或原始业务载荷。长期 alerting/TSDB、Prometheus/Grafana/OTel exporter、自动 remediation 和对外 status 承诺不属于本设计。


## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 验证方法 / 精确来源 / candidate 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) | [新鲜度归约](#report-freshness) | 失败与样本年龄、实际覆盖分别表达 | 流量 schema 2 已接通实际 reducer/adapter；[traffic summary 回归](../../../scripts/traffic-monitor-summary.test.sh)覆盖持续失败、稀疏端点、时钟回退、旧 observation 与历史 as-of；外层持久尝试另行验收 | 实现 PR #5009 的本地／CI 结果；后续条件发布实现 PR 的同候选断言 | 不证明单独 collector 的持久尝试、runtime 或现场健康 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[角色观测](node-triad-operations-observability.prd.md#ops-profile-readiness) | [外层持久尝试](#durable-observability-attempt) | 采集前 pending、严格递增序号、同尝试条件终态与验证读取 | [实际编排回归](../../../scripts/p2p-real-env-observability-attempt.test.py)：实际入口／helper；SIGKILL、A 慢／B 新、落盘失败、状态回退和恢复正例 | 当前实现 PR 的 source HEAD、本地／CI 命令与断言结果 | 不证明独立 collector、角色权威就绪、真实 SSH 或物理回滚 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[角色观测](node-triad-operations-observability.prd.md#ops-profile-readiness) | [host 持久尝试](#host-durable-attempt) | host 独立排序、类型绑定与条件发布 | [host 实际入口回归](../../../scripts/p2p-real-env-host-durable.test.py)：真实 wrapper／发布 helper 的并发、中断、故障与恢复，另执行外层兼容及消费链 | 当前实现 PR 的 source HEAD、本地／CI 命令与 JSON／Markdown 断言 | 不证明其他 collector、角色就绪、真实 SSH 或物理回滚 |
| Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[角色观测](node-triad-operations-observability.prd.md#ops-profile-readiness) | [角色就绪投影](#report-profile-readiness) | OPS-AC-5 与失败终态指针不掩盖阻断 | planned：扩展 [observability 回归](../../../scripts/p2p-real-env-observability-monitor.test.sh)，走 collector→adapter→report，注入失效 grant、未采用成员、签名失败、P/R 故障、容量和旧样本，以及 A 慢/B 快、pending 中止与终态写入失败；现有 fixture 不覆盖该编排链 | 未来实现 PR 的同候选输出、退出码和各角色断言；当前仅设计 | 不证明协议撤销/BFT/物理故障域或任意拓扑已支持 |
| [独占 run](node-triad-operations-observability.prd.md#observability-run-contract) | [编排目标](#run-exclusive) | 外层写入前原子分配独占 run，同 run 工件关联一致 | [实际编排回归](../../../scripts/p2p-real-env-observability-attempt.test.py)：固定时钟并发、输出别名和预存工件／内部 symlink 拒绝 | 当前实现 PR 的本地／CI 命令、退出码与断言 | 不证明独立 snapshot 等入口的目录隔离、真实 SSH 或现场恢复 |
| [参数路由](node-triad-operations-observability.prd.md#observability-run-contract) | [编排目标](#run-routing) | 三节点参数按消费者分配，非法参数采集与写入前失败 | [实际路由回归](../../../scripts/p2p-real-env-observability-routing.test.py)：严格 argv 消费检查与真实子工具离线集成，覆盖 service／status／target／storage 等单项及组合、help、缺值与不支持拓扑 | 当前实现 PR 的 source HEAD、本地／CI 命令、退出码及 JSON／Markdown 断言 | 不证明任意拓扑、host 内部失败续采、producer 修复、真实 SSH 或现场恢复 |
| [失败后报告](node-triad-operations-observability.prd.md#observability-run-contract) | [编排目标](#run-failure-report) | 节点失败不跳过后续采样，保留本 run 失败及非零终态 | [host 实际采集回归](../../../scripts/p2p-real-env-host-failure.test.py)：首个／后续／全部失败、部分响应、失败后恢复及真实 reducer／merged 消费；另执行现有持久尝试回归 | 当前实现 PR 的 source HEAD、本地／CI 命令及 JSON／Markdown 断言 | 不证明独立 collector 持久尝试、目录隔离、真实 SSH 或物理故障域恢复 |
| [持久错误](node-triad-operations-observability.prd.md#observability-run-contract) | [编排目标](#run-worker-error) | reward collector/probe state 写入失败独立保留 last_error/degraded，不因旧 report、无新 epoch 或 invariant 正常清错。 | 当前只核对 `../../../scripts/p2p-real-env-observability-summary.py`（summary） 的 summarize_reward_runtime 消费边界；producer 写入实现未在本专题核实。未来由 worker owner 沿实际调用链提供连续两轮失败/真正恢复 fixture，再核对 status→report；不得从 summary 通过推导写入恢复。  离线工具验证遵循 [testing manual](../../../testing-manual.md)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 未运行目标故障/并发回归；不证明 producer 修复、真实 SSH 或现场恢复。 |
| [选用范围](node-triad-operations-observability.prd.md#ops-selected-scope) | [报告目标](#report-scope) | 本轮明确候选、inventory、用途；旧 fleet 退出不把旧未验证结果升级为通过。 | 核对选定配置与报告身份；未来离线 fixture 检查不同 inventory 与候选一致性，现场证据另按当次授权取得。  离线工具验证遵循 [testing manual](../../../testing-manual.md)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。 |
| [OPS-AC-1](node-triad-operations-observability.prd.md#ops-report-contract) | [报告目标](#report-envelope) | 真实 traffic envelope 保值，未知 schema/不可用/缺文件显式诊断，仅支持格式可 adapter。 | 静态审读 summary 的 summarize_traffic_control_plane 与 fixture 断言；未来新增真实 single-node envelope→adapter→merged Markdown 正负例。现有 fixture aggregate 不证明全部 envelope 适配。  离线工具验证遵循 [testing manual](../../../testing-manual.md)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。 |
| [OPS-AC-2](node-triad-operations-observability.prd.md#ops-report-contract) | [报告目标](#report-freshness) | 最新 attempt、最近成功、窗口截止独立；显式 as-of 保留合法历史且不伪造当前覆盖。 | 未来执行本地矩阵 00:00/00:10 成功后 00:11–00:30 失败及恢复/全失败/reset/历史 as-of；审读 traffic monitor 采集接线与 summary 消费字段，已有 full_window_covered 断言不证明 producer/reducer 新鲜度。  离线工具验证遵循 [testing manual](../../../testing-manual.md)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。 |
| [OPS-AC-3](node-triad-operations-observability.prd.md#ops-report-contract) | [报告目标](#report-percentile) | 真实窗口 nearest-rank p99 与 max、样本量分别表达。 | 未来走 producer→adapter→status→report，验证 127×100ms+1×1000ms 的 p99=100ms、max=1000ms，以及 100/128/空窗/滚动淘汰；summary 手工字段不是 producer 算法证明。当前未取得该链运行证据。  离线工具验证遵循 [testing manual](../../../testing-manual.md)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。 |
| [OPS-AC-4](node-triad-operations-observability.prd.md#ops-report-contract) | [报告目标](#report-storage) | storage 目录并集、GC 状态与不足信息按上游解释，报告不授予发布或 GC 正确性。 | 按 `../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md#storage-observation-contract`（storage observation） 的目录重叠、GC 六状态、degraded 正负例核对 status→summarize_storage→JSON/Markdown；独立 gate 与真实 release 接线分别验收，当前未运行。  离线工具验证遵循 [testing manual](../../../testing-manual.md)；源码审读与未来实际执行分别记录。 | 同一候选的静态核对记录；未来实际命令、退出码、断言与适用环境分项记录 | 现有 fixture 仅覆盖其断言子范围；不证明完整 producer/adapter 链、现场健康或 release readiness。 |
