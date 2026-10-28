# P2P 共识文档导航

本目录汇总两个已完成、但权威边界不同的共识专题。先按问题选专题；不要把完成状态当作当前 P2P runtime、网络 readiness 或发布结论。

## 从这里开始

- 想了解 builtin Wasm 如何用跨平台一致的模块身份、签名和 identity manifest 替代“所有宿主产物字节 hash 必须相同”的假设：先读 `builtin-wasm-identity-consensus.prd.md`。
- 想了解如何把节点侧重复的 PoS 共识纯逻辑收敛到 `oasis7_consensus`、保持 node 负责网络与运行时接线：先读 `consensus-code-consolidation-to-oasis7-consensus.prd.md`。
- 想确认这两个已完成专题的历史任务、依赖和回归证据：从对应的 GitHub Issue / GitHub Project 进入。
- 想了解当前模块级 P2P / consensus / finality / readiness 口径：回到 `doc/p2p/prd.md` 与 GitHub Issue / GitHub Project；精确文件名检索使用 `doc/p2p/prd.index.md`。

## 权威边界

| 专题 | 负责的权威问题 | 不负责的结论 |
| --- | --- | --- |
| `builtin-wasm-identity-consensus.*` | builtin Wasm 的模块身份、identity manifest、跨平台构建校验与本地回退边界 | 修改 Wasm ABI、发布级 Docker builder，或当前 runtime/readiness 结论 |
| `consensus-code-consolidation-to-oasis7-consensus.*` | 共识纯逻辑向 `oasis7_consensus` 的 crate 边界和回归收敛 | 重写共识算法、完整 fork-choice/finality 升级，或当前网络 readiness 结论 |

## 阅读与维护约定

- `*.prd.md` 是专题规格；`*.design.md` 解释设计；GitHub Issue / GitHub Project 保存完成任务与回归追溯。
- 本页只承担首次分流和边界说明，不复制专题规格、命令或完成记录。
- 新增共识专题时，更新本页的分流与边界，并保留 `doc/p2p/prd.index.md` 中的精确 triplet 行；共享目录规则以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
