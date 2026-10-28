# DistFS 子域入口

## 从这里开始
- 想确认 DistFS 在 P2P/consensus/execution/observer 组合后的整体闭环、测试层级与 claim boundary：先读 `testing-manual.md#s9a链上大世界状态底座自闭环`；本页只负责 DistFS 子域入口。
- 想理解 DistFS production hardening 的当前合同：先读 `distfs-production-hardening.prd.md`，再按需进入同名 design/project。
- 想查分布式韧性与自愈边界：先读 `distfs-distributed-resilience.prd.md`；其中 NodeRuntime 轮询接线仍作为受限的增量能力，保留“缺依赖跳过、单轮失败不阻断 tick”的边界。
- 想查公开反馈账本及其 announce/fetch 复制：先读 `distfs-feedback-ledger-and-replication.prd.md`；它收敛历史 open-ledger、P2P bridge 与 NodeRuntime integration 三个专题，并明确 replication、BlobState lane 和非就绪边界。
- 想查标准 FileStore、本地索引与 blob 回收边界：进入 `distfs-production-hardening.prd.md`；想查 builtin WASM artifact hydrate/hash/loader：进入 `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md`。早期 net path-index/Observer/Bootstrap 的七个 source files 已删除，历史实现仅保留于 Git history；它们都不是当前 active API。

## 阅读面边界
- 本页只做 DistFS 子域分流，不复制 `doc/p2p/prd.index.md` 的完整三件套长表。
- DistFS 专题的 blob closure、复制、恢复或 self-healing green 结果只能作为链上大世界状态底座的存储层证据；模块级闭环以 S9A 的 `module_required / module_full / integration_required / release_full` 分层为准。
- `distfs-production-hardening` 是 production-hardening 的唯一当前专业权威，收敛历史 Phase 1-9 的本地 CAS/索引完整性与 storage challenge 调度合同。原 phase 文件名仅保留在历史 audit/review 文字和 Git history 中，不作为当前入口或 readiness 依据。
- `distfs-production-hardening` 同时承接早期标准 FileStore I/O 的当前实现边界；builtin artifact 工件治理由 World Runtime WASM 专业 authority 承接。早期 runtime path-index 与 observer/bootstrap 文档已退役：七个旧 net source files 仅在 Git history 中，历史合同不构成恢复能力。
- distributed-resilience 是异构 provider、无单机完整依赖与自愈组的默认主入口；轮询与 NodeRuntime 接线只承载受限增量差异。
- 完整文件级检索仍回到 `doc/p2p/prd.index.md`。

## 专题入口
| 组 | 默认入口 | 边界 |
| --- | --- | --- |
| DistFS production hardening | `distfs-production-hardening.prd.md` | 收敛 Phase 1-9 的历史完成态；不替代节点存储奖励池结算、跨节点 challenge/proof 协议或 production/readiness 证据。 |
| DistFS distributed resilience | `distfs-distributed-resilience.prd.md` | 异构 provider 选择、无单机完整依赖、自愈轮询与受限 NodeRuntime 接线统一收敛；NodeRuntime 仅在依赖齐备时执行，单轮失败不阻断 tick。 |
| DistFS feedback ledger and replication | `distfs-feedback-ledger-and-replication.prd.md` | 公开 append-only 反馈、作者签名、announce/fetch 复制及有界 runtime 接线；不绑定共识/finality，也不是 readiness 证据。 |

## 维护规则
- 新增 DistFS 专题时，先判断是否属于既有专业权威；历史完成态应语义吸收到对应当前专题并保留 provenance，不新建默认 phase 入口。
- 新增内容若引入新的通用边界，应先拆出新主入口，再更新本页分流。
- 三个 2026-03 feedback 源三件套已在语义回填和活跃引用修复后删除；追溯仅使用 Git 与 `.pm` task evidence。当前首读与专业合同以 `distfs-feedback-ledger-and-replication` 三件套为准。
- 六组 2026-02 DistFS/network closure 源三件套的有效语义已分别归入本页 current authority、World Runtime WASM authority 与 P2P 根负向边界；过时 API/完成态不保留为当前入口。
