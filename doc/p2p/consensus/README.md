# P2P 共识文档导航

本目录汇总共识相关的已完成专题与中期目标演进设计。先按问题选专题；不要把文档完成或目标设计当作当前 P2P runtime、网络 readiness 或发布结论。

## 从这里开始

- 想了解 builtin Wasm 如何用跨平台一致的模块身份、签名和 identity manifest 替代“所有宿主产物字节 hash 必须相同”的假设：先读 `builtin-wasm-identity-consensus.prd.md`。
- 想了解如何把节点侧重复的 PoS 共识纯逻辑收敛到 `oasis7_consensus`、保持 node 负责网络与运行时接线：先读 `consensus-code-consolidation-to-oasis7-consensus.prd.md`。
- 想了解 BFT 受保护签名器如何从数十 MiB 复合状态瘦身为 `< 4 KiB` 最小防双签可信基（Minimal TCB），并将大体积私有候选保留、流量计费账与轮次超时时钟解耦回 Node 层：先读 [`bft-minimal-signer-tcb.design.md`](bft-minimal-signer-tcb.design.md)。
- 想确认已完成专题的历史任务、依赖和回归证据：从对应的 GitHub Issue / GitHub Project 进入。
- 想了解当前模块级 P2P / consensus / finality / readiness 口径：回到 `doc/p2p/prd.md` 与 GitHub Issue / GitHub Project；精确文件名检索使用 `doc/p2p/prd.index.md`。

## 权威边界

| 专题 | 负责的权威问题 | 不负责的结论 |
| --- | --- | --- |
| `builtin-wasm-identity-consensus.*` | builtin Wasm 的模块身份、identity manifest、跨平台构建校验与本地回退边界 | 修改 Wasm ABI、发布级 Docker builder，或当前 runtime/readiness 结论 |
| `consensus-code-consolidation-to-oasis7-consensus.*` | 共识纯逻辑向 `oasis7_consensus` 的 crate 边界和回归收敛 | 重写共识算法、完整 fork-choice/finality 升级，或当前网络 readiness 结论 |
| [`bft-minimal-signer-tcb.design.md`](bft-minimal-signer-tcb.design.md) | BFT 受保护签名器 `< 4 KiB` 最小可信基、私有保留大对象/流量账外置锚定与共识超时时钟分离的中期目标架构 | 替代现行 `wire=3`（`#5518`）交付义务，或声称精简接口与迁移已实现 |

## 阅读与维护约定

- `*.prd.md` 是专题规格；`*.design.md` 解释设计（承接模块级 `doc/p2p/prd.md` 的技术演进可独立使用 `*.design.md`）；GitHub Issue / GitHub Project 保存完成任务与回归追溯。
- 本页只承担首次分流和边界说明，不复制专题规格、命令或完成记录。
- 新增共识专题时，更新本页的分流与边界，并按适用规则同步 `doc/p2p/prd.index.md`；共享目录规则以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
