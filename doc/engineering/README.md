# engineering 文档索引

## 入口
- PRD: `doc/engineering/prd.md`
- 设计总览: `doc/engineering/design.md`
- 通用开发流程：[开发流程规范](workflow/source-of-truth.md)；执行事实：当前 PR、实际 CI 与评审记录（Issue 按需）
- 文件级索引: `doc/engineering/prd.index.md`

## 从这里开始
- 想看工程治理边界、验收条件与长期规则：`doc/engineering/prd.md`
- 想看当前治理工作与结果：进入对应 PR、实际 CI 与评审记录（Issue 按需）；项目级派生摘要从[项目状态](command-center/README.md)进入并核对其来源窗口。
- 想按专题进入具体治理文档：`doc/engineering/prd.index.md`

## 模块职责
- 维护工程治理规则、文档组织标准与执行门禁。
- 跟踪文档迁移、文件级索引可达性与角色协作规范。
- 承接 engineering 治理趋势、季度审查与模板化流程沉淀。

## 按主题进入
- 私有机器、环境与操作资料工具：从 [产品需求](private-context/private-context.prd.md) 进入，格式、离线历史与恢复见 [系统设计](private-context/private-context.design.md)
- 想了解项目长期/短期目标、目前进度、近期变化和待关注事项：从[项目状态](command-center/README.md)进入，关键部署与设计边界见[架构索引](command-center/architecture/README.md)，日常维护见[持续更新流程](command-center/command-center.manual.md)
- 想开发云端指挥中心或本地辅助端：从[产品设计](command-center/command-center.prd.md)进入，职责拆分、云端个人记录、来源接入与恢复合同见[系统设计](command-center/command-center.design.md)
- 文档治理、入口减重、存量维护成本、目录职责与 redirect 规则：统一从 `doc/engineering/doc-governance/README.md` 分流
- 环境分层、云上清单、仓库健康巡检与季度复核：统一从 `doc/engineering/governance/README.md` 分流
- Rust 文件结构、Workspace 治理、架构边界与协议产物：统一从 [Rust 工程治理入口](rust-governance/README.md) 分流
- 历史迁移、审读记录与文件级专题检索：从 `doc/engineering/prd.index.md` 下钻；当前运行型工程治理资料从 `doc/engineering/governance/README.md` 下钻

## 共享约定
- 模块根入口、专题落位、README 职责与 legacy redirect 的共享治理规则统一从 `doc/engineering/doc-governance/README.md` 进入，再按问题下钻到规范正文或对应专题。
- 共享规则与专题长表统一回收到 `doc/engineering/prd.index.md` 与各专题 PR、实际 CI 与评审记录（Issue 按需），本页只保留 landing 所需分流。
