# `world-simulator/scenario` 文档入口

本目录汇集世界初始化、场景文件、seed/location 与 asteroid-fragment 生成专题。首次阅读先按要回答的问题选择一个专题；不要把已完成的命名迁移或专项基线误读成当前世界运行态，当前执行状态以 PR 与实际验证记录 为准。

## 从这里开始

| 需要回答的问题 | 首读专题 | 边界 |
| --- | --- | --- |
| 当前可用场景、稳定断言和场景加载契约是什么？ | `scenario-files.prd.md` | 场景文件与验证矩阵的入口；不替代模块级执行状态。 |
| 新世界如何初始化，基础、多地点与 P2P 场景各服务什么验证？ | `world-initialization.prd.md` | 初始化场景矩阵与用途；不是 production readiness 结论。 |
| seed、地点和 deterministic spawn 如何生成？ | `scenario-seed-locations.prd.md` | location/agent 初始化专题；统一资源 manifest 语义继续读 `unified-world-seed-fragment-runtime.prd.md`。 |
| chunk、frag 资源预算、补种与 onboarding 如何衔接？ | `chunked-fragment-generation.prd.md` | 生成、预算和补种主入口；首局资源选择读 `../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md`，初始位置继续读 `agent-frag-initial-spawn-position.prd.md`。 |
| asteroid fragment 的规范命名、override、spacing、设施基线和 replay 边界在哪里？ | `scenario-files.prd.md` + `chunked-fragment-generation.prd.md` | 场景 authority 负责 effective config 与显式设施注入；生成 authority 负责 spacing、跨 chunk 确定性和 committed-delta replay。 |

## 阅读与维护边界

- 每个专题的 `*.prd.md`、`*.design.md` 分别保存规格和设计；执行证据、任务状态与阻断由 GitHub task issue / Project 保存。本页只负责簇级首读分流，不复制其参数、测试或完成状态。
- 模块范围、当前执行状态和精确文件检索分别回到 `../prd.md`、GitHub task issue / Project 和 `../prd.index.md`。
- 现存专题只有在没有明确现行替代物或仍承担专业合同、测试/运维证据时保留；已完成且可由稳定权威完整承接的碎片，应在语义合并和活跃引用清零后删除。
- 新增或退休 scenario 专题时：更新本页的首读分流与边界、保留 `../prd.index.md` 的精确 PRD/design 行；共享目录规则以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
