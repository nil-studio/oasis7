# oasis7 三节点运维观测合同

- 对应设计文档: `doc/p2p/node/node-triad-operations-observability.design.md`
- 执行与验证事实: Git、PR、实际 CI 与评审记录；Issue/Project 按需使用。

## 权威边界

本专题拥有被实际选用的 real-environment triad 工具之 operator 与 evidence 合同，不声明任何历史 fleet 当前仍在线或必须继续部署。它定义节点 inventory、health/status/host/process/traffic/WASM 采样、bounded artifact、summary 解释和 fail-closed 行为；runtime 的 consensus、status schema 与 replay 语义仍由对应 runtime authority 定义，QA 决定 release/evidence verdict。

它不声明主网、public-chain-grade reachability、长期告警平台、自动 deploy/restart/rollback/restore/state-sync 或 release readiness。一次 status read、fixture 或 monitor run 不创建、续期或升级运行证据。

## 目标

提供一个可重复执行、可审计且 fail-closed 的 triad operator/evidence 入口，使同一窗口可以同时定位链健康、宿主机资源、traffic、WASM 与 bounded reachability 的异常。

## 范围

覆盖 repo-owned monitor、inventory、host/service 与 status 输入、summary 产物、fixture 验证和 evidence interpretation；不覆盖 consensus 实现、自动 remediation、外部监控平台或发布放行。

## 接口 / 数据

`/healthz`、`/v1/chain/status`、host/process sample、traffic/WASM window、`overall.status`、per-node alerts、module breakdown 和 `optimization_candidates` 是本专题的接口与 artifact contract。私钥、seed、SSH password、完整 env 秘密和原始业务载荷不得进入产物。

## Inventory 与采样合同

三节点工具沿用 `local_node`、`sequencer_ecs`、`storage_ecs` 兼容标签；实际 runtime role 必须从同一窗口的 sampled status 读取，不能由标签推断。默认 service labels 为 `oasis7-triad-observer.service`、`oasis7-triad-sequencer.service`、`oasis7-triad-storage.service`，部署 inventory 可以显式覆盖。

| 输入 | 当前合同 | fail-closed 行为 |
| --- | --- | --- |
| host/process | CPU、load、memory、storage、systemd service、runtime PID/CPU/memory/thread 采样 | SSH 不可达、PID 缺失、service 无法读取或 storage 不可读必须成为 node alert。 |
| chain health | `/healthz`、`/v1/chain/status`、height/progress、consensus、replication、storage、reward、transactions 与 execution/state-root projection | snapshot 不是 `pass_candidate` 时 summary 至少为 `blocked`，不得被资源正常覆盖。 |
| windows | traffic 与 WASM cumulative snapshot 以同一 run-dir 汇总为窗口 summary | reset/restart 缩短窗口必须显式标识；history 按窗口和 buffer bounded retention。 |
| reachability | 消费 status 的 bounded path projection、active path mix、fallback reason 与 confidence | 不重新推导 canonical path truth，不导出 unbounded peer-id labels，不把 control-plane bytes 当 NIC overhead。 |

## 产物与解释

Canonical command is `scripts/p2p-real-env-observability-monitor.sh`; it composes triad snapshot, host monitor, traffic monitor and WASM monitor. It writes machine-readable JSON and Markdown summary containing `overall.status`、per-node alerts、module breakdown and `optimization_candidates`.

`pass_with_resource_alerts` 或 `pass_with_module_alerts` 仅可在 underlying snapshot 为 `pass_candidate` 时使用。summary 的可读诊断不构成 public-chain claim、长期可用性承诺或发布许可。

<a id="observability-run-contract"></a>
## 单次编排与失败诊断

- 写任何配置、样本或 report 之前，snapshot 与外层 monitor 必须原子取得独占 run 目录；可分配不同目录，或使冲突调用在写前明确失败。秒级/更高精度时间戳和 `mkdir -p` 本身不保证独占，不能截断预存 sentinel 或另一调用工件。
- 同一 run 的 inventory、host/traffic/WASM 输入与最终报告保持所属一致；只读取该次产物，不把旧 `latest_summary` 冒充本轮。
- host SSH 在首次、后续或全部节点采样失败时，仍保留该次带准确 label、采集时间、不可用状态和受限错误类别的失败样本，继续收集可用节点，最终生成 host 与 merged JSON/Markdown 诊断并非零退出。不得以正常数值填缺数，也不得覆盖 underlying snapshot 的 blocked。
- 先解析 canonical CLI 参数，再按消费者传递：service 覆盖只给 snapshot/host，status URL 只给 snapshot/traffic，共享 target 只给适用者；非法选项须在采集前拒绝，help 与操作说明一致。
- reward worker 的 collector/probe state 持久化失败是独立诊断事实：完整轮末 `last_error`/degraded 与告警仍须可见，已有 report、没有新 epoch 或 invariant 正常均不能清除此错误。best-effort 数值仍可更新；只有对应写入真正恢复成功后才清错，不根据其他步骤成功推定恢复。

本节规定应满足的编排/诊断合同，不把文档修订作为脚本或 worker 修复证明。离线验收分别覆盖首个/后续/全部 SSH 失败、固定时钟同 out-dir 并发与预存 sentinel、不同 inventory、service/status URL 单项和组合、非法参数写前失败，以及连续两轮写失败/真正恢复后清错；使用实际 orchestration 的 stub collectors，不只测 summary helper，无需真实 SSH/节点。

<a id="ops-selected-scope"></a>
## 当次选用范围与历史环境退出

先明确实际使用的环境、模式、用途、节点集合及一个候选版本；不从旧标签、IP、包版本或过往停机记录推断今天的状态。
本专题的 triad 工具只在实际选用其受支持三节点范围时验收；其他单权威/多节点路径按所属合同验证，
不因本页名称获得支持声明，也不把旧三/五/六节点拓扑作为工业首局的新前置。

旧 fleet 不再使用时，记录其退出范围和理由即可停止旧候选逐轮追验；未验证的历史结果不改为通过。
继续使用时，仅对当次选定 candidate 核包/BUILDINFO/checksum、实际 service ExecStart/readback、适用 readiness、
health/NRestarts 和同窗推进。报告、工具回归和现场部署分开交付，离线工具修复无需等待旧 fleet。
已有 consumer-impact、observer hold 与禁止操作仍有效；范围调整不授权重启、清理、恢复或对外 all-clear。

<a id="ops-report-contract"></a>
## 报告准确性与新鲜度验收

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；本节采集新鲜度子范围承接 [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)。技术报告不能把历史成功投影为当前消费者可用性；完整消费者服务闸门仍由对应 authority 判定。

- OPS-AC-1：真实 single-node traffic envelope 的 bytes/window/reset/last_error 能进入 merged Markdown；未知 schema、available=false 或缺文件显式不可用，不静默填 0。仅支持的旧平铺格式可走显式 adapter。
- OPS-AC-2：最新 attempt、最近成功及窗口截止分别表达。成功后持续失败不能沿用旧成功窗口宣称当前完整覆盖；离线重算使用明确 as-of，不强制套用当前墙钟，也不抹掉有界历史成功。
  - 当前流量子范围按[新鲜度策略](node-triad-operations-observability.design.md#traffic-freshness-v2)实现统一 as-of、最新尝试／最近成功、采样年龄／缺口和合并复核；旧报告缺证据保持 unknown。持久 pending 与角色就绪仍未由该子范围实现。
- OPS-AC-3：p99 使用实际采样窗口和明确算法；128 个样本中 127×100ms＋1×1000ms 的 nearest-rank p99 为 100ms，max 为 1000ms。样本不足不能以 max 冒充已满足完整窗口的 p99。
- OPS-AC-4：容量与 GC 结果按 [存储指标及 Gate 合同](../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md#storage-observation-contract) 解释；报告告警不等于发布授权，也不证明 GC 执行机制正确。

以上只定义当前使用这些工具时的正确输出。JSON、Markdown 与终端的事实和窗口保持一致；现场全 fleet 健康、
对外恢复、只读可用和指定写服务恢复各需其适用证据，不能用单节点/validator pair 健康相互替代。

<a id="ops-profile-readiness"></a>
## 按实际角色解释节点就绪

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；局部承接 [REQ-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) 与 [AC-DCS-005](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-005) 的节点观测义务。

目标报告分开表达进程存活、当次采集、最后成功、已验证读取和投票/追加资格。实际 profile、world/chain、实例与构建身份必须来自受支持的同窗状态；旧标签、PID、HTTP 200 或缺省值不替代。未知 schema、缺必需字段、时钟异常及陈旧样本均显示具体不可判定原因。没有适用字段的旧 profile 显式标为 unsupported 或该字段不适用，不以空值制造绿色。

| 实际角色 | 必须保留的诊断语义 | 不能据此推导 |
| --- | --- | --- |
| 公开投影 observer | 已验证投影位置/根、最后观察时间、追赶差距、证明阻断 | 已装载完整私有 World 或拥有写权 |
| 全状态只读节点 | 已验证且实际装载的 cut/root、head lag、grant 到期/撤销状态、authority epoch、来源、历史/状态容量与恢复耗时 | 当前 validator membership、投票或 archive 保证 |
| controlled writer / P/R | 最近 Qualified 提交及其年龄、双端持久与保护服务状态、epoch/fencing、未决原请求和写入阻断 | 单端故障仍可继续写入或已实现 BFT |
| BFT validator 目标 | 治理决定/排程位置/实际采用的 set digest 分离，round/height、最近签名与提交、签名持久化和投票阻断 | 配置了 validator 名称或读取同步就等于投票就绪 |

OPS-AC-5：相同进程存活样本分别注入 grant 失效、成员未采用、签名持久化失败、单持久端不可用、容量到限与采集陈旧，报告必须保留对应角色的阻断；恢复只在 producer 的适用证据真实更新后显示。实现这些字段需要对应 runtime producer 与适配链支持；本文不把现有 triad 工具声明为任意拓扑监控平台，也不新增自动重启或外部告警服务。

## 验证与恢复边界

- `bash -n scripts/p2p-real-env-node-host-sample.sh scripts/p2p-real-env-host-monitor.sh scripts/p2p-real-env-observability-monitor.sh`
- `python3 -m py_compile scripts/p2p-real-env-host-summary.py scripts/p2p-real-env-observability-summary.py`
- `bash scripts/p2p-real-env-host-monitor.test.sh`
- `bash scripts/p2p-real-env-observability-monitor.test.sh`

真实环境采样、部署、restart、rollback、restore 与 state sync 需要独立授权和每次环境证据。restart 只能是诊断或临时恢复，不能替代对代码、配置或部署根因的修复。

## 里程碑

- M1：host/process、snapshot、traffic、WASM 与 merged summary contract 落地。
- M2：fixture 和 canonical command 保持可运行，持续积累 separately authorized real-environment evidence。

## 风险

- 远端系统命令格式、SSH 或 service state 的差异可能使输入缺失；必须显式 alert。
- 将局部资源、relay 或 control-plane 指标误读为 public-chain/release verdict 会扩大本专题边界。
