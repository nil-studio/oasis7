# README production 文档入口

本目录收纳仍有独立语义的 README 生产收口专题，以及仍需保留来源与发布边界的历史素材。它不是当前实现或运营动作的首读入口。

## 先读哪里

- 想确认当前 README 对外口径与长期要求：先读 `doc/readme/README.md` 和 `doc/readme/prd.md`；实际进展回到对应 PR、CI 与评审记录（已有 Task 时沿用）。
- 想按专题精确检索、区分活跃与历史压缩项：读 `doc/readme/prd.index.md`。
- 只在需要追溯生产收口的设计决策、验收证据或关联素材时，再进入本目录。

## 本目录分层

- **历史保留专题**：`readme-prod-gap1245-wasm-repl-topology-player.*` 在模块索引中归入历史保留项；需求、约束与现行去向见[专题 PRD 的历史边界](readme-prod-gap1245-wasm-repl-topology-player.prd.md#historical-retention-and-routing)（旧兼容跳转 design 已移除）。本页不据此声明全部完成或开启新的执行队列。
- **已完成的专业权威合并**：旧 `readme-llm-p1p2-production-closure.*`、`readme-p0-p1-closure.*` 与 `readme-prod-closure-llm-distfs-consensus.*` 只重复已完成的 LLM、runtime、共识、DistFS 与 viewer 实现任务。当前产品承诺从 `doc/product/` 四模块树进入，专业规则和实现真值从 `doc/world-simulator/llm/`、`doc/world-runtime/`、`doc/p2p/` 与 `doc/world-simulator/viewer/` 进入；viewer 不拥有节点、拓扑或共识门控真值。旧三件套已删除，历史任务从 Git history 与 GitHub task evidence 追溯。
- **未发布的历史草稿与素材**：`xiaohongshu-ai-economy-report-draft-2026-06-18.md` 及 `assets/xhs-ai-economy-2026-06-18/` 保留文案、分镜和素材来源关系。当前 PRD 未列出 PRD-README-052；不能据此推定完成或获准发布。草稿注明原始研究材料未入库，来源核验状态未知；复用前须按其现有 LiveOps/claims review 边界重新核验，不能仅因日期较旧删除。

## 收敛规则

- 不在此页平铺每份三件套或素材文件；文件级精确清单只维护在 `doc/readme/prd.index.md`。
- 新 production 专题先更新模块 `prd.md` / PR、实际 CI 与评审记录（Issue 按需），再在索引中标记其活跃或历史压缩状态；本页只维护分流与边界。
- 做迁移治理时，应逐项把仍有效语义合并到现行权威、修复引用并删除已被完整吸收的源文件；只有仍承担独立专业操作或未闭环状态时才保留，并写明 authority reason。
