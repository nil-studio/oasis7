# scripts 文档入口

审计轮次: 11

本页只提供脚本模块的权威文档入口。脚本参数、工作流门禁和
任务完成状态不在这里复制，避免入口页与实现或规范分别演进。

## 按意图选择入口

| 目标 | 权威入口 |
| --- | --- |
| 理解脚本模块的职责、分层、接口与验收标准 | `doc/scripts/prd.md` |
| 理解模块结构和各文档层的关系 | `doc/scripts/design.md` |
| 查看脚本模块任务映射与完成记录 | 对应 PR、实际 CI 与评审记录；已有 Task Issue 按实际范围承接 |
| 按文件名查找仍独立维护的专题文档 | `doc/scripts/prd.index.md` |
| 理解治理主题如何归入稳定模块权威 | `doc/scripts/governance/README.md` |
| 选择 pre-commit 当前契约或修复流程 | `doc/scripts/precommit/README.md` |
| 当前发布级 deterministic WASM pipeline | `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md` |
| 旧 nightly / build-std 参数、CI 约束和 hash 漂移背景（历史，不是发布入口） | `doc/world-runtime/wasm/evidence.md#historical-nightly-build-std-provenance` |
| 执行仓库任务生命周期 | `doc/engineering/workflow/source-of-truth.md` |
| 选择测试套件与本地验证路径 | `testing-manual.md` |

## 真值归属

- `doc/scripts/prd.md` 定义脚本能力的模块级需求和稳定契约，不承担任务状态。
- 对应 PR/已有 Task 记录本次适用要求、实际验证版本与结果；PRD 保留稳定要求和验证方法，不作为任务身份或完成记录。
- `doc/scripts/governance/README.md` 只解释治理主题的稳定权威归属；当前能力与兼容边界归入 `doc/scripts/{prd,design}.md`，历史任务从原 PR/Task 与 Git history 追溯。
- `doc/engineering/workflow/source-of-truth.md` 是工程任务生命周期、状态和门禁的唯一规范。脚本模块文档只描述相关 helper 的能力，不另建一套流程。
- 脚本自身的 `--help` 和测试是参数及机器可读输出的当前实现证据；入口页不固化易漂移的参数清单、端口或内部步骤。

## 文档树约束

- 模块根目录只保留 `README.md`、`prd.md`、`design.md` 和 `prd.index.md`；任务过程归对应 PR、实际 CI 与评审记录（Issue 按需）。
- 新脚本专题按 `governance/`、`precommit/` 或既有匹配主题目录落位；WASM 发布设计与任务归入 world-runtime canonical pipeline，历史参数更正只回写其 evidence，不恢复旧 nightly 路由。不要在模块根目录新增专题文件。
- 新增、移动或退役仍需独立维护的专题时，同步更新对应专题 README 与 `doc/scripts/prd.index.md`。
- 历史完成记录留在 PR、实际 CI 与评审记录（Issue 按需）或 evidence 文档；已被当前规范吸收的一次性交接说明不再作为 live 导航入口。
